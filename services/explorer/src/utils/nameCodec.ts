const SAFE_INITIAL = /^[A-Za-z_]$/;
const SAFE_SUBSEQUENT = /^[A-Za-z0-9_-]$/;

const requireWellFormedName = (value: string): void => {
  if (value.length === 0) {
    throw new Error('Name cannot be empty');
  }
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) {
        throw new Error('Name contains invalid Unicode');
      }
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      throw new Error('Name contains invalid Unicode');
    }
  }
};

/** Encode a user-facing name as one reader-safe Scheme symbol. */
export const encodeSafeName = (value: string): string => {
  requireWellFormedName(value);
  const bytes = new TextEncoder().encode(value);
  let encoded = '';
  bytes.forEach((byte, index) => {
    const character = String.fromCharCode(byte);
    const safe = (index === 0 ? SAFE_INITIAL : SAFE_SUBSEQUENT).test(character);
    encoded += safe ? character : `%${byte.toString(16).toUpperCase().padStart(2, '0')}`;
  });
  return encoded;
};

/** Decode the application convention for display without changing stored paths. */
export const decodeSafeName = (value: string): string => {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
};
