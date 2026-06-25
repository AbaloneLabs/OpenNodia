export function orderLinkUiState(detail, error = '') {
  if (error) {
    return { kind: 'error', labelKey: 'dex.orderLinkInvalid', canAct: false };
  }
  if (!detail) {
    return { kind: 'idle', labelKey: 'dex.orderLinkNeedsReview', canAct: false };
  }

  if (!detail.payload_valid || !detail.canonical_escrow_match) {
    return { kind: 'invalid', labelKey: 'dex.orderLinkTampered', canAct: false };
  }

  const status = String(detail.status || '');
  if (status.includes('expired') || detail.verification?.expired) {
    return { kind: 'expired', labelKey: 'dex.orderLinkExpired', canAct: false };
  }

  if (detail.verification && !detail.verification.valid) {
    return { kind: 'warning', labelKey: 'dex.orderLinkLedgerInactive', canAct: false };
  }

  if (detail.verification?.valid) {
    return { kind: 'verified', labelKey: 'dex.orderLinkVerified', canAct: true };
  }

  return { kind: 'warning', labelKey: 'dex.orderLinkNeedsReview', canAct: false };
}
