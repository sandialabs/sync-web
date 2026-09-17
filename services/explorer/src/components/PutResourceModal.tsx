import React, { useEffect, useRef, useState } from 'react';
import { JournalService, PutStorageMode, ResourcePutInput } from '../services/JournalService';

interface PutResourceModalProps {
  open: boolean;
  onClose: () => void;
  onSubmit: (name: string, input: ResourcePutInput) => Promise<void>;
}

const defaultBody = (mode: PutStorageMode): string => {
  if (mode === 'object') return '(define-class (resource)\n  (define-method (*init* self) #t))';
  if (mode === 'bytes') return '#u()';
  if (mode === 'expression') return '()';
  return '';
};

const PutResourceModal: React.FC<PutResourceModalProps> = ({ open, onClose, onSubmit }) => {
  const [name, setName] = useState('');
  const [mode, setMode] = useState<PutStorageMode>('string');
  const [body, setBody] = useState(defaultBody('string'));
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const nameRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) return;
    setName('');
    setMode('string');
    setBody(defaultBody('string'));
    setError(null);
    setSubmitting(false);
    window.setTimeout(() => nameRef.current?.focus(), 0);
  }, [open]);

  if (!open) return null;

  const changeMode = (next: PutStorageMode) => {
    setMode(next);
    setBody(defaultBody(next));
    setError(null);
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError('Name is required');
      return;
    }
    if (JournalService.isReservedStateSegment(trimmedName)) {
      setError('Names wrapped in * are reserved');
      return;
    }
    try {
      const input: ResourcePutInput = mode === 'string'
        ? { mode, textValue: body }
        : { mode, schemeValue: body };
      if (mode !== 'string' && !body.trim()) throw new Error('Scheme body is required');
      setSubmitting(true);
      setError(null);
      await onSubmit(trimmedName, input);
      onClose();
    } catch (submitError) {
      setError(submitError instanceof Error ? submitError.message : 'Put failed');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="resource-modal-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget && !submitting) onClose();
    }}>
      <section className="resource-modal" role="dialog" aria-modal="true" aria-labelledby="put-resource-title">
        <div className="resource-modal-header">
          <div>
            <div className="resource-kicker">Create only</div>
            <h2 id="put-resource-title">Put resource</h2>
          </div>
          <button type="button" className="button-inline-icon" aria-label="Close Put resource" onClick={onClose}>x</button>
        </div>
        <form onSubmit={submit}>
          <label className="resource-field">
            <span>Name</span>
            <input ref={nameRef} value={name} onChange={(event) => setName(event.target.value)} />
          </label>
          <fieldset className="resource-choice-set">
            <legend>Storage</legend>
            {(['string', 'bytes', 'expression', 'object'] as PutStorageMode[]).map((choice) => (
              <label key={choice}>
                <input type="radio" name="storage" checked={mode === choice} onChange={() => changeMode(choice)} />
                {choice[0].toUpperCase() + choice.slice(1)}
              </label>
            ))}
          </fieldset>
          <label className="resource-field resource-body-field">
            <span>{mode === 'string' ? 'Text' : 'Complete Scheme body'}</span>
            <textarea value={body} onChange={(event) => setBody(event.target.value)} spellCheck={false} />
          </label>
          <p className="resource-help">
            {mode === 'string' && 'Ordinary text is stored as its exact UTF-8 byte vector.'}
            {mode === 'bytes' && 'Provide one exact Scheme byte-vector. No text, JSON, hex, or base64 conversion is performed.'}
            {mode === 'expression' && 'Provide one complete Scheme expression.'}
            {mode === 'object' && 'Provide one complete Scheme define-class value. The stored shell remains uninitialized.'}
          </p>
          <p className="resource-help">The target must be absent; an existing resource is never overwritten.</p>
          {error && <div className="content-action-error" role="alert">{error}</div>}
          <div className="resource-modal-actions">
            <button type="button" className="button button-secondary" disabled={submitting} onClick={onClose}>Cancel</button>
            <button type="submit" className="button button-primary" disabled={submitting}>{submitting ? 'Putting...' : 'Put'}</button>
          </div>
        </form>
      </section>
    </div>
  );
};

export default PutResourceModal;
