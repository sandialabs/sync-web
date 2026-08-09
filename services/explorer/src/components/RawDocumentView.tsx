import React, { useEffect, useMemo, useState } from 'react';

interface RawDocumentViewProps {
  content: unknown;
  filename?: string;
}

type PreviewKind = 'image' | 'svg' | 'pdf' | 'audio' | 'video' | 'text' | 'binary';

export const byteVectorBytes = (content: unknown): Uint8Array | null => {
  if (!content || typeof content !== 'object' || Array.isArray(content)) return null;
  const hex = (content as Record<string, unknown>)['*type/byte-vector*'];
  if (typeof hex !== 'string' || hex.length % 2 !== 0 || /[^0-9a-f]/i.test(hex)) return null;
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
};

const startsWith = (bytes: Uint8Array, signature: number[], offset = 0): boolean =>
  signature.every((byte, index) => bytes[offset + index] === byte);

const ascii = (bytes: Uint8Array, start: number, length: number): string =>
  String.fromCharCode(...Array.from(bytes.slice(start, start + length)));

const validUtf8 = (bytes: Uint8Array): string | null => {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
};

const extension = (filename?: string): string => {
  const match = filename?.toLowerCase().match(/\.([a-z0-9]+)$/);
  return match?.[1] ?? '';
};

const escapedText = (text: string): string =>
  Array.from(text).map((character) => {
    const code = character.charCodeAt(0);
    if (character === '\n' || character === '\r' || character === '\t') return character;
    if (code < 0x20 || code === 0x7f) return `\\x${code.toString(16).padStart(2, '0')}`;
    return character;
  }).join('');

export const detectRawPreview = (
  bytes: Uint8Array,
  filename?: string,
): { kind: PreviewKind; mime: string; text?: string } => {
  const text = validUtf8(bytes);
  const ext = extension(filename);

  if (startsWith(bytes, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) {
    return { kind: 'image', mime: 'image/png' };
  }
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) {
    return { kind: 'image', mime: 'image/jpeg' };
  }
  if (ascii(bytes, 0, 6) === 'GIF87a' || ascii(bytes, 0, 6) === 'GIF89a') {
    return { kind: 'image', mime: 'image/gif' };
  }
  if (ascii(bytes, 0, 4) === 'RIFF' && ascii(bytes, 8, 4) === 'WEBP') {
    return { kind: 'image', mime: 'image/webp' };
  }
  if (ascii(bytes, 4, 4) === 'ftyp' && ['avif', 'avis'].includes(ascii(bytes, 8, 4))) {
    return { kind: 'image', mime: 'image/avif' };
  }
  if (ascii(bytes, 0, 2) === 'BM') {
    return { kind: 'image', mime: 'image/bmp' };
  }
  if (startsWith(bytes, [0x00, 0x00, 0x01, 0x00])) {
    return { kind: 'image', mime: 'image/x-icon' };
  }

  if (text !== null && /^(?:\s*<\?xml[^>]*>\s*)?<svg(?:\s|>)/i.test(text)) {
    return { kind: 'svg', mime: 'image/svg+xml', text };
  }

  if (bytes.slice(0, 1024).findIndex((_, index) => ascii(bytes, index, 5) === '%PDF-') >= 0) {
    return { kind: 'pdf', mime: 'application/pdf' };
  }

  if (ascii(bytes, 0, 4) === 'RIFF' && ascii(bytes, 8, 4) === 'WAVE') {
    return { kind: 'audio', mime: 'audio/wav' };
  }
  if (ascii(bytes, 0, 4) === 'OggS') {
    return { kind: ext === 'ogv' ? 'video' : 'audio', mime: ext === 'ogv' ? 'video/ogg' : 'audio/ogg' };
  }
  if (ascii(bytes, 0, 3) === 'ID3'
      || (bytes.length > 1 && bytes[0] === 0xff && (bytes[1] & 0xe0) === 0xe0)) {
    return { kind: 'audio', mime: 'audio/mpeg' };
  }
  if (bytes.length > 1 && bytes[0] === 0xff && (bytes[1] & 0xf0) === 0xf0) {
    return { kind: 'audio', mime: 'audio/aac' };
  }
  if (startsWith(bytes, [0x1a, 0x45, 0xdf, 0xa3])) {
    return { kind: 'video', mime: 'video/webm' };
  }
  if (ascii(bytes, 4, 4) === 'ftyp') {
    const audio = ['m4a', 'm4b', 'aac'].includes(ext);
    return { kind: audio ? 'audio' : 'video', mime: audio ? 'audio/mp4' : 'video/mp4' };
  }

  // Textual HTML, XML, JavaScript, and other source stays inert in <pre>.
  if (text !== null) {
    return { kind: 'text', mime: 'text/plain', text };
  }

  return { kind: 'binary', mime: 'application/octet-stream' };
};

const RawDocumentView: React.FC<RawDocumentViewProps> = ({ content, filename }) => {
  const bytes = useMemo(() => byteVectorBytes(content), [content]);
  const preview = useMemo(
    () => bytes ? detectRawPreview(bytes, filename) : null,
    [bytes, filename],
  );
  const [objectUrl, setObjectUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!bytes || !preview) {
      setObjectUrl(null);
      return undefined;
    }
    const url = URL.createObjectURL(new Blob([bytes], { type: preview.mime }));
    setObjectUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [bytes, preview]);

  if (!bytes || !preview) {
    return <div className="empty-state">Raw view is available only for stored byte-vector values.</div>;
  }

  const hex = Array.from(bytes.slice(0, 128))
    .map((byte) => byte.toString(16).padStart(2, '0')).join(' ');

  return (
    <div className="raw-document">
      <div className="raw-document-meta">
        <span>{bytes.length} bytes</span>
        <span>{preview.mime}</span>
        {objectUrl && <a href={objectUrl} download={filename || 'value'}>Download exact bytes</a>}
      </div>
      {(preview.kind === 'text' || preview.kind === 'svg') && (
        <pre className="content-text">{escapedText(preview.text || '')}</pre>
      )}
      {preview.kind === 'image' && objectUrl && (
        <img className="raw-media-preview" src={objectUrl} alt="Raw value preview" />
      )}
      {preview.kind === 'svg' && objectUrl && (
        <img className="raw-media-preview raw-svg-preview" src={objectUrl} alt="Isolated SVG preview" />
      )}
      {preview.kind === 'pdf' && objectUrl && (
        <iframe className="raw-pdf-preview" src={objectUrl} title="PDF preview" sandbox="allow-same-origin" />
      )}
      {preview.kind === 'audio' && objectUrl && (
        <audio className="raw-audio-preview" src={objectUrl} controls />
      )}
      {preview.kind === 'video' && objectUrl && (
        <video className="raw-media-preview" src={objectUrl} controls />
      )}
      {preview.kind === 'binary' && (
        <div className="raw-binary-fallback">
          <p>Browser preview is unavailable for this byte-vector value.</p>
          <pre className="content-text">{hex}{bytes.length > 128 ? ' …' : ''}</pre>
        </div>
      )}
    </div>
  );
};

export default RawDocumentView;
