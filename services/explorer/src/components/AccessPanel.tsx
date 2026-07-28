import React, { FormEvent, useEffect, useState } from 'react';
import { JournalService } from '../services/JournalService';
import { AuthorizationRule, JournalPath } from '../types';
import './AccessPanel.css';

interface AccessPanelProps {
  journalService: JournalService;
  currentUser: string;
  refreshKey: number;
  isAdmin?: boolean;
}

const splitPathInput = (value: string): JournalPath => value.trim().split(/\s+/).filter(Boolean);
const parseNamespaceInput = (value: string): JournalPath => {
  const parts = splitPathInput(value);
  if (parts.length === 0) throw new Error('Managed namespace is required.');
  return parts.length === 1 ? ['*state*', parts[0]] : parts;
};
const namespaceDisplay = (path: JournalPath) => `(${path.join(' ')})`;
const safeNamespaceDisplay = (value: string, fallbackUser: string, isAdmin: boolean) => {
  if (!isAdmin) return namespaceDisplay(['*state*', fallbackUser]);
  try {
    return namespaceDisplay(parseNamespaceInput(value));
  } catch {
    return '(invalid namespace)';
  }
};
const formatPath = (path: JournalPath) => path.length === 0 ? '(root)' : path.join(' / ');
const formatPrincipal = (principal: JournalPath) => principal.join(' ');
const formatResolve = (value: AuthorizationRule['resolve']) => (
  Array.isArray(value) ? `${value[0]} … ${value[1]}` : value ? 'all indices' : 'disabled'
);

const parseResolve = (value: string): AuthorizationRule['resolve'] => {
  const normalized = value.trim().toLowerCase();
  if (normalized === '' || normalized === 'false' || normalized === 'off') return false;
  if (normalized === 'true' || normalized === 'all') return true;
  const parts = normalized.split(/\s+/).map((part) => Number.parseInt(part, 10));
  if (parts.length === 2 && parts.every(Number.isInteger)) return [parts[0], parts[1]];
  throw new Error('Resolve must be off, all, or two integer indices like “100 -1”.');
};

const emptyRuleForm = {
  principal: '',
  path: '',
  get: false,
  set: false,
};

const AccessPanel: React.FC<AccessPanelProps> = ({ journalService, currentUser, refreshKey, isAdmin = false }) => {
  const [rules, setRules] = useState<AuthorizationRule[]>([]);
  const [form, setForm] = useState(emptyRuleForm);
  const [namespaceInput, setNamespaceInput] = useState(currentUser);
  const [resolveEnabled, setResolveEnabled] = useState(false);
  const [resolveStart, setResolveStart] = useState('');
  const [resolveEnd, setResolveEnd] = useState('');
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const managedNamespace = () => (isAdmin ? parseNamespaceInput(namespaceInput) : ['*state*', currentUser]);

  const loadRules = async () => {
    setIsLoading(true);
    setError(null);
    try {
      setRules(await journalService.getAuthorizations(managedNamespace()));
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : 'Could not load access rules');
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void loadRules();
  }, [journalService, currentUser, refreshKey, namespaceInput, isAdmin]);

  const buildRule = (): AuthorizationRule => {
    const principal = splitPathInput(form.principal);
    if (principal.length === 0) throw new Error('Share with is required.');
    if (resolveEnabled && (!resolveStart.trim() || !resolveEnd.trim())) {
      throw new Error('Resolve requires both start and end indices.');
    }
    const resolve = resolveEnabled ? parseResolve(`${resolveStart} ${resolveEnd}`) : false;
    if (Array.isArray(resolve) && resolve[0] < 0 && resolve[1] >= 0) {
      throw new Error('Resolve cannot use a relative start with an absolute end.');
    }
    if (!form.get && !form.set && resolve === false) {
      throw new Error('Select at least one allowed function.');
    }
    return {
      principal,
      path: splitPathInput(form.path),
      get: form.get,
      'set!': form.set,
      resolve,
    };
  };

  const handleAddRule = async (event: FormEvent) => {
    event.preventDefault();
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      const rule = buildRule();
      if (rule.principal.length === 0) throw new Error('Principal is required.');
      await journalService.authorize(managedNamespace(), rule);
      setForm(emptyRuleForm);
      setResolveEnabled(false);
      setResolveStart('');
      setResolveEnd('');
      await loadRules();
      setStatus('Access rule added.');
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not add access rule');
    } finally {
      setIsSaving(false);
    }
  };

  const handleDeleteRule = async (rule: AuthorizationRule) => {
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await journalService.deauthorize(managedNamespace(), rule);
      await loadRules();
      setStatus('Access rule removed.');
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not remove access rule');
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="access-panel">
      {(error || status) && (
        <div className={`access-message ${error ? 'error' : 'success'}`}>{error ?? status}</div>
      )}

      <section className="access-card">
        <div className="access-card-header">
          <div>
            <h2>Add rule</h2>
            <p>Rules are local to this journal, apply under <code>{safeNamespaceDisplay(namespaceInput, currentUser, isAdmin)}</code>, and are checked before federated reads, writes, and resolves.</p>
          </div>
        </div>
        <form className="access-form" onSubmit={handleAddRule}>
          {isAdmin && (
            <label className="access-wide-field">
              Manage namespace
              <input value={namespaceInput} onChange={(event) => setNamespaceInput(event.target.value)} placeholder="example: bob or *state* bob" disabled={isSaving} />
            </label>
          )}
          <label className="access-wide-field">
            Share with principal
            <input value={form.principal} onChange={(event) => setForm({ ...form, principal: event.target.value })} placeholder="example: peer *state* alice" disabled={isSaving} />
          </label>
          <label className="access-wide-field">
            Path under your namespace
            <input value={form.path} onChange={(event) => setForm({ ...form, path: event.target.value })} placeholder="example: docs project" disabled={isSaving} />
          </label>
          <div className="access-toggles" aria-label="Allowed functions">
            <label className="access-switch-row">resolve <input type="checkbox" checked={resolveEnabled} onChange={(event) => setResolveEnabled(event.target.checked)} disabled={isSaving} /><span className="access-switch" /></label>
            <label className="access-switch-row">get <input type="checkbox" checked={form.get} onChange={(event) => setForm({ ...form, get: event.target.checked })} disabled={isSaving} /><span className="access-switch" /></label>
            <label className="access-switch-row">set! <input type="checkbox" checked={form.set} onChange={(event) => setForm({ ...form, set: event.target.checked })} disabled={isSaving} /><span className="access-switch" /></label>
          </div>
          <fieldset className="access-resolve-range">
            <label>
              Start index
              <input type="number" value={resolveStart} onChange={(event) => setResolveStart(event.target.value)} placeholder="example: 100" disabled={isSaving || !resolveEnabled} />
            </label>
            <label>
              End index
              <input type="number" value={resolveEnd} onChange={(event) => setResolveEnd(event.target.value)} placeholder="example: -1" disabled={isSaving || !resolveEnabled} />
            </label>
          </fieldset>
          <button className="button button-primary access-form-submit" type="submit" disabled={isSaving}>
            Add
          </button>
        </form>
      </section>

      <section className="access-card">
        <div className="access-card-header">
          <div>
            <h2>Current rules</h2>
            <p>All rules are recursive for <code>{safeNamespaceDisplay(namespaceInput, currentUser, isAdmin)}</code>.</p>
          </div>

        </div>
        {isLoading && rules.length === 0 ? (
          <div className="access-empty">Loading access rules…</div>
        ) : rules.length === 0 ? (
          <div className="access-empty">No explicit access rules yet. This namespace is private except to its owner and admins.</div>
        ) : (
          <div className="access-rule-list">
            {rules.map((rule, index) => (
              <article className="access-rule" key={`${formatPrincipal(rule.principal)}-${formatPath(rule.path)}-${index}`}>
                <div className="access-rule-main">
                  <div className="access-rule-principal">{formatPrincipal(rule.principal)}</div>
                  <div className="access-rule-path">{formatPath(rule.path)}</div>
                </div>
                <div className="access-rule-permissions">
                  {rule.get && <span>get</span>}
                  {rule['set!'] && <span>set!</span>}
                  {rule.resolve !== false && <span>resolve: {formatResolve(rule.resolve)}</span>}
                </div>
                <button className="button button-primary" type="button" onClick={() => void handleDeleteRule(rule)} disabled={isSaving}>
                  Delete
                </button>
              </article>
            ))}
          </div>
        )}
      </section>
    </div>
  );
};

export default AccessPanel;
