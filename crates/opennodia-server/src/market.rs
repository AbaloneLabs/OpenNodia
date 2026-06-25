//! Market price fetching (ALGO/USD from Coingecko).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cached price quote.
#[derive(Debug, Clone)]
pub struct PriceQuote {
    pub price_usd: f64,
    pub change_24h: f64,
    pub fetched_at: Instant,
}

/// A historical ALGO/USD price point.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PriceHistoryPoint {
    pub timestamp: u64,
    pub price_usd: f64,
}

/// Cached historical quote series.
#[derive(Debug, Clone)]
pub struct PriceHistoryQuote {
    pub points: Vec<PriceHistoryPoint>,
    pub fetched_at: Instant,
}

/// Price cache with TTL — avoids hammering the external API.
#[derive(Clone)]
pub struct PriceCache {
    inner: Arc<PriceCacheInner>,
}

struct PriceCacheInner {
    cache: RwLock<Option<PriceQuote>>,
    history_cache: RwLock<HashMap<u32, PriceHistoryQuote>>,
    ttl: Duration,
    history_ttl: Duration,
    client: reqwest::Client,
}

impl PriceCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(PriceCacheInner {
                cache: RwLock::new(None),
                history_cache: RwLock::new(HashMap::new()),
                ttl,
                history_ttl: ttl.max(Duration::from_secs(10 * 60)),
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .user_agent("OpenNodia/0.1 (self-hosted Algorand node app)")
                    .build()
                    .expect("reqwest client"),
            }),
        }
    }

    /// Get the current ALGO price, fetching from Coingecko if stale.
    /// Returns `None` if the price can't be determined (offline, API down).
    pub async fn get_algo_price(&self) -> Option<PriceQuote> {
        // Check cache first.
        {
            let guard = self.inner.cache.read().await;
            if let Some(ref quote) = *guard {
                if quote.fetched_at.elapsed() < self.inner.ttl {
                    return Some(quote.clone());
                }
            }
        }

        // Cache miss or stale — fetch fresh.
        match self.fetch_coingecko().await {
            Ok(quote) => {
                *self.inner.cache.write().await = Some(quote.clone());
                tracing::debug!(price = quote.price_usd, "fetched fresh ALGO price");
                Some(quote)
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to fetch ALGO price");
                // Return stale cache if we have one, even if expired.
                self.inner.cache.read().await.clone()
            }
        }
    }

    /// Get historical ALGO prices, fetching from Coingecko if stale.
    pub async fn get_algo_history(&self, days: u32) -> Option<PriceHistoryQuote> {
        let days = days.clamp(1, 365);
        {
            let guard = self.inner.history_cache.read().await;
            if let Some(quote) = guard.get(&days) {
                if quote.fetched_at.elapsed() < self.inner.history_ttl {
                    return Some(quote.clone());
                }
            }
        }

        match self.fetch_coingecko_history(days).await {
            Ok(quote) => {
                self.inner
                    .history_cache
                    .write()
                    .await
                    .insert(days, quote.clone());
                tracing::debug!(
                    days,
                    points = quote.points.len(),
                    "fetched ALGO price history"
                );
                Some(quote)
            }
            Err(e) => {
                tracing::warn!(days, error = %e, "failed to fetch ALGO price history");
                self.inner.history_cache.read().await.get(&days).cloned()
            }
        }
    }

    async fn fetch_coingecko(&self) -> anyhow::Result<PriceQuote> {
        let url = "https://api.coingecko.com/api/v3/simple/price?ids=algorand&vs_currencies=usd&include_24hr_change=true";
        let resp = self
            .inner
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await?;

        if !status.is_success() {
            tracing::warn!(%status, body = %body, "coingecko returned non-success status");
            anyhow::bail!("coingecko returned {status}");
        }

        let price_usd = body["algorand"]["usd"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("missing usd price in response: {body}"))?;
        let change_24h = body["algorand"]["usd_24h_change"].as_f64().unwrap_or(0.0);

        Ok(PriceQuote {
            price_usd,
            change_24h,
            fetched_at: Instant::now(),
        })
    }

    async fn fetch_coingecko_history(&self, days: u32) -> anyhow::Result<PriceHistoryQuote> {
        let url = "https://api.coingecko.com/api/v3/coins/algorand/market_chart";
        let resp = self
            .inner
            .client
            .get(url)
            .header("Accept", "application/json")
            .query(&[
                ("vs_currency", "usd".to_string()),
                ("days", days.clamp(1, 365).to_string()),
            ])
            .send()
            .await?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await?;

        if !status.is_success() {
            tracing::warn!(%status, body = %body, "coingecko history returned non-success status");
            anyhow::bail!("coingecko history returned {status}");
        }

        Ok(PriceHistoryQuote {
            points: parse_history_points(&body)?,
            fetched_at: Instant::now(),
        })
    }
}

fn parse_history_points(body: &serde_json::Value) -> anyhow::Result<Vec<PriceHistoryPoint>> {
    let prices = body
        .get("prices")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing prices array in history response"))?;
    let mut points = Vec::with_capacity(prices.len());
    for entry in prices {
        let Some(pair) = entry.as_array() else {
            continue;
        };
        if pair.len() < 2 {
            continue;
        }
        let Some(timestamp_ms) = pair[0].as_f64() else {
            continue;
        };
        let Some(price_usd) = pair[1].as_f64() else {
            continue;
        };
        if !price_usd.is_finite() || timestamp_ms < 0.0 {
            continue;
        }
        points.push(PriceHistoryPoint {
            timestamp: (timestamp_ms / 1000.0).floor() as u64,
            price_usd,
        });
    }
    if points.is_empty() {
        anyhow::bail!("history response did not contain usable prices");
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_history_points_skips_invalid_entries() {
        let body = serde_json::json!({
            "prices": [
                [1700000000000.0, 0.12],
                ["bad", 0.13],
                [1700003600000.0, 0.14]
            ]
        });
        let points = parse_history_points(&body).unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].timestamp, 1_700_000_000);
        assert_eq!(points[1].price_usd, 0.14);
    }
}
