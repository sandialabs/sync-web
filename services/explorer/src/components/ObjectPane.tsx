import React, { useEffect, useState } from 'react';
import { JournalPath } from '../types';
import { JournalService, ObjectInvocationResult } from '../services/JournalService';

interface ObjectPaneProps {
  journalService: JournalService;
  path: JournalPath;
  historical: boolean;
  metadata: string;
  apiResult: ObjectInvocationResult;
}

const exactOutput = (value: unknown): string => {
  const text = JournalService.byteVectorToText(value);
  return text ?? (typeof value === 'string' ? value : JSON.stringify(value, null, 2));
};

const ObjectPane: React.FC<ObjectPaneProps> = ({
  journalService, path, historical, metadata, apiResult,
}) => {
  const [method, setMethod] = useState('');
  const [argumentsExpression, setArgumentsExpression] = useState('()');
  const [readOnly, setReadOnly] = useState(true);
  const [output, setOutput] = useState<ObjectInvocationResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [executing, setExecuting] = useState(false);
  useEffect(() => {
    setOutput(null);
    setError(null);
    setMethod('');
    setArgumentsExpression('()');
    setReadOnly(true);
  }, [path, historical]);

  const execute = async (event: React.FormEvent) => {
    event.preventDefault();
    setExecuting(true);
    setError(null);
    setOutput(null);
    try {
      const result = await journalService.invokeObject({
        path,
        method: method.trim(),
        argumentsExpression,
        readOnly: historical || readOnly,
        historical,
      });
      setOutput(result);
    } catch (executeError) {
      setError(executeError instanceof Error ? executeError.message : 'Object call failed');
    } finally {
      setExecuting(false);
    }
  };

  const operation = historical ? 'retrieve' : 'use!';
  const federation = journalService.getFederationContext() ?? { route: [] };
  const context = `((operation ${operation})\n`
    + ` (path ${JournalService.pathToScheme(path)})\n`
    + ` (method ${method.trim() || '()'})\n`
    + ` (arguments ${argumentsExpression.trim() || '()'})\n`
    + ` (route ${JournalService.pathToScheme(federation.route.map((name) => ({
      '*type/string*': name,
    })))})\n`
    + ` (history-indexes (${(federation.historyIndexes ?? []).join(' ')}))\n`
    + ` (read-only? ${historical || readOnly ? '#t' : '#f'})\n`
    + ` (persistence ${historical || readOnly ? 'none' : 'stage-successor'}))`;

  return (
    <section className="object-pane" aria-labelledby="object-pane-title">
      <div className="object-pane-heading">
        <div>
          <div className="resource-kicker">Scheme object boundary</div>
          <h3 id="object-pane-title">Object</h3>
        </div>
        <span className="object-operation-chip">{operation}</span>
      </div>

      <div className="object-inspection">
        <h4>Blank-call metadata / digests</h4>
        <pre>{metadata}</pre>
        <h4>Advertised API</h4>
        <pre>{exactOutput(apiResult.result)}</pre>
      </div>

      <form className="object-call-form" onSubmit={execute}>
        <label className="resource-field">
          <span>Method symbol</span>
          <input value={method} onChange={(event) => setMethod(event.target.value)} placeholder="increment!" />
        </label>
        <label className="resource-field resource-body-field">
          <span>Complete Scheme arguments list</span>
          <textarea value={argumentsExpression} onChange={(event) => setArgumentsExpression(event.target.value)} spellCheck={false} />
        </label>
        <label className="object-read-only-control">
          <input
            type="checkbox"
            checked={historical || readOnly}
            disabled={historical}
            onChange={(event) => setReadOnly(event.target.checked)}
          />
          Read only
        </label>
        <button type="submit" className="button button-primary" disabled={executing}>
          {executing ? 'Executing…' : `Execute ${operation}`}
        </button>
      </form>

      <details className="object-context" open>
        <summary>Execution context</summary>
        <pre>{context}</pre>
      </details>
      {error && <div className="content-action-error" role="alert">{error}</div>}
      {output && (
        <div className="object-output" aria-live="polite">
          <h4>Exact result</h4>
          <pre>{exactOutput(output.result)}</pre>
        </div>
      )}
    </section>
  );
};

export default ObjectPane;
