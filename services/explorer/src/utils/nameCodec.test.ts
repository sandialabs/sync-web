import { decodeSafeName, encodeSafeName } from './nameCodec';

describe('safe name codec', () => {
  const names = [
    'ordinary-name',
    'two words',
    '.',
    '#t',
    'peer|reader',
    'peer\\reader',
    'path/name',
    '雪',
    '%',
    '%2E',
    '123',
  ];

  it.each(names)('round trips %p', (name) => {
    expect(decodeSafeName(encodeSafeName(name))).toBe(name);
  });

  it('uses uppercase escapes and preserves compatible safe names', () => {
    expect(encodeSafeName('ordinary-name_2')).toBe('ordinary-name_2');
    expect(encodeSafeName('a/b.c')).toBe('a%2Fb%2Ec');
    expect(encodeSafeName('雪')).toBe('%E9%9B%AA');
  });

  it('keeps literal percent spellings collision-free', () => {
    expect(encodeSafeName('.')).toBe('%2E');
    expect(encodeSafeName('%2E')).toBe('%252E');
    expect(encodeSafeName('.')).not.toBe(encodeSafeName('%2E'));
  });

  it('produces distinct canonical encodings for adversarial names', () => {
    expect(new Set(names.map(encodeSafeName)).size).toBe(names.length);
  });

  it('rejects empty and malformed Unicode names', () => {
    expect(() => encodeSafeName('')).toThrow('Name cannot be empty');
    expect(() => encodeSafeName('\ud800')).toThrow('Name contains invalid Unicode');
    expect(() => encodeSafeName('\udc00')).toThrow('Name contains invalid Unicode');
  });
});
