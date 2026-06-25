const MAX_SAFE_RAW = BigInt(Number.MAX_SAFE_INTEGER);

export function parseDecimalToRaw(value, decimals) {
  const text = String(value ?? '').trim();
  if (!Number.isInteger(decimals) || decimals < 0) {
    throw new Error('invalid asset decimals');
  }
  if (!/^\d+(?:\.\d*)?$/.test(text)) {
    throw new Error('invalid amount');
  }

  const [wholePart, fractionPart = ''] = text.split('.');
  if (fractionPart.length > decimals) {
    throw new Error(`amount supports at most ${decimals} decimal places`);
  }

  const scale = 10n ** BigInt(decimals);
  const fraction = fractionPart.padEnd(decimals, '0');
  const raw = BigInt(wholePart) * scale + BigInt(fraction || '0');
  if (raw <= 0n) {
    throw new Error('amount must be greater than zero');
  }
  return raw;
}

export function rawToSafeNumber(raw) {
  const value = typeof raw === 'bigint' ? raw : BigInt(raw);
  if (value < 0n || value > MAX_SAFE_RAW) {
    throw new Error('amount exceeds the supported safe integer range');
  }
  return Number(value);
}

export function multiplyDecimalsToRaw(left, right, decimals) {
  if (!Number.isInteger(decimals) || decimals < 0) {
    throw new Error('invalid asset decimals');
  }
  const parse = (value) => {
    const text = String(value ?? '').trim();
    if (!/^\d+(?:\.\d*)?$/.test(text)) {
      throw new Error('invalid amount or price');
    }
    const [whole, fraction = ''] = text.split('.');
    return {
      value: BigInt(`${whole}${fraction}`),
      scale: 10n ** BigInt(fraction.length),
    };
  };
  const lhs = parse(left);
  const rhs = parse(right);
  const numerator = lhs.value * rhs.value * 10n ** BigInt(decimals);
  const denominator = lhs.scale * rhs.scale;
  if (numerator % denominator !== 0n) {
    throw new Error(`total supports at most ${decimals} decimal places`);
  }
  const raw = numerator / denominator;
  if (raw <= 0n) {
    throw new Error('amount must be greater than zero');
  }
  return raw;
}

export function formatRawAmount(raw, decimals) {
  const value = typeof raw === 'bigint' ? raw : BigInt(raw ?? 0);
  if (!Number.isInteger(decimals) || decimals < 0) {
    throw new Error('invalid asset decimals');
  }
  if (decimals === 0) return value.toString();

  const scale = 10n ** BigInt(decimals);
  const whole = value / scale;
  const fraction = (value % scale)
    .toString()
    .padStart(decimals, '0')
    .replace(/0+$/, '');
  return fraction ? `${whole}.${fraction}` : whole.toString();
}

export function utf8ByteLength(value) {
  return new TextEncoder().encode(String(value ?? '')).length;
}
