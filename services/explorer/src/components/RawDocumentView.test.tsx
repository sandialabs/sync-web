import { TextDecoder, TextEncoder } from 'util';
import { byteVectorBytes, detectRawPreview } from './RawDocumentView';

Object.assign(global, { TextDecoder, TextEncoder });

const bytes = (...values: number[]): Uint8Array => new Uint8Array(values);

describe('RawDocumentView detection', () => {
  it('extracts exact byte vectors', () => {
    expect(Array.from(byteVectorBytes({ '*type/byte-vector*': '00ff41' }) || []))
      .toEqual([0, 255, 65]);
    expect(byteVectorBytes({ '*type/byte-vector*': 'xyz' })).toBeNull();
  });

  it('keeps HTML, XML, JavaScript, and control-heavy UTF-8 inert', () => {
    const html = new TextEncoder().encode('<script>alert(1)</script>\u0000');
    const xml = new TextEncoder().encode('<?xml version="1.0"?><root/>');
    expect(detectRawPreview(html, 'page.html')).toMatchObject({
      kind: 'text', mime: 'text/plain',
    });
    expect(detectRawPreview(xml, 'data.xml')).toMatchObject({
      kind: 'text', mime: 'text/plain',
    });
  });

  it('recognizes allowlisted image signatures without stored MIME data', () => {
    const png = bytes(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a);
    const jpeg = bytes(0xff, 0xd8, 0xff, 0xdb);
    const gif = new TextEncoder().encode('GIF89a');
    const webp = new TextEncoder().encode('RIFF0000WEBP');
    expect(detectRawPreview(png, 'wrong.jpg')).toEqual({ kind: 'image', mime: 'image/png' });
    expect(detectRawPreview(jpeg).mime).toBe('image/jpeg');
    expect(detectRawPreview(gif).mime).toBe('image/gif');
    expect(detectRawPreview(webp).mime).toBe('image/webp');
  });

  it('recognizes a PDF header in the initial range', () => {
    const pdf = new TextEncoder().encode('\n%PDF-1.7\n');
    expect(detectRawPreview(pdf, 'unknown.bin').kind).toBe('pdf');
    expect(detectRawPreview(new TextEncoder().encode('%PDF'), 'report.pdf').kind)
      .toBe('text');
  });

  it('recognizes browser-native audio and video signatures', () => {
    const mp3 = bytes(0x49, 0x44, 0x33, 0x04);
    const webm = bytes(0x1a, 0x45, 0xdf, 0xa3);
    const mp4 = new TextEncoder().encode('0000ftypisom');
    expect(detectRawPreview(mp3).kind).toBe('audio');
    expect(detectRawPreview(webm).kind).toBe('video');
    expect(detectRawPreview(mp4, 'clip.mp4').kind).toBe('video');
    expect(detectRawPreview(mp4, 'song.m4a').kind).toBe('audio');
  });

  it('allows SVG only as source plus an isolated image preview', () => {
    const svg = new TextEncoder().encode('<svg><script>alert(1)</script></svg>');
    expect(detectRawPreview(svg, 'image.svg')).toMatchObject({
      kind: 'svg', mime: 'image/svg+xml',
    });
  });

  it('uses an octet-stream fallback for unknown binary bytes', () => {
    expect(detectRawPreview(bytes(0, 0xff, 0, 0xfe), 'payload.html')).toEqual({
      kind: 'binary', mime: 'application/octet-stream',
    });
  });
});
