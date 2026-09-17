import React, { FormEvent, useEffect, useRef, useState } from 'react';
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
  if (parts.length === 1 && parts[0] !== '*state*' && parts[0] !== '*public*') {
    return ['*state*', parts[0]];
  }
  throw new Error('Managed namespace must be one local username.');
};
const namespaceDisplay = (path: JournalPath) => String(path[1] ?? '');
const parseHumanPathInput = (value: string): JournalPath => {
  const segments: string[] = [];
  let segment = '';
  let quoted = false;
  let closedQuote = false;
  let escaped = false;
  const finish = () => {
    if (segment.length === 0) throw new Error('Path segments cannot be empty.');
    segments.push(JournalService.encodePathSegment(segment));
    segment = '';
    closedQuote = false;
  };
  for (const character of value) {
    if (escaped) {
      if (!/[\s"\\]/.test(character)) {
        throw new Error('Path backslash escapes support only whitespace, quote, or backslash.');
      }
      segment += character;
      escaped = false;
    } else if (character === '\\') {
      escaped = true;
    } else if (quoted) {
      if (character === '"') {
        quoted = false;
        closedQuote = true;
      } else {
        segment += character;
      }
    } else if (character === '"') {
      if (segment.length > 0 || closedQuote) {
        throw new Error('A quoted path segment must begin after whitespace.');
      }
      quoted = true;
    } else if (/\s/.test(character)) {
      if (segment.length > 0 || closedQuote) finish();
    } else {
      if (closedQuote) throw new Error('A closing quote must be followed by whitespace.');
      segment += character;
    }
  }
  if (escaped) throw new Error('Path cannot end with an incomplete backslash escape.');
  if (quoted) throw new Error('Path has an unclosed quote.');
  if (segment.length > 0 || closedQuote) finish();
  return segments;
};
const formatPath = (path: JournalPath) => path.length === 0
  ? '(home)'
  : path.map((segment) => typeof segment === 'string'
    ? JournalService.decodePathSegment(segment)
    : String(segment)).join(' / ');
const principalPresentation = (principal: JournalPath) => {
  if (principal.length === 1 && principal[0] === '*public*') {
    return { kind: 'Public', journal: '', user: '' };
  }
  const stateIndex = principal.lastIndexOf('*state*');
  return {
    kind: stateIndex > 0 ? 'Remote user' : 'Local user',
    journal: stateIndex > 0 ? principal.slice(0, stateIndex).join(' ') : '',
    user: stateIndex >= 0 ? String(principal[stateIndex + 1] ?? '') : '',
  };
};
const formatPrincipal = (principal: JournalPath) => {
  const presentation = principalPresentation(principal);
  if (presentation.kind === 'Public') return 'Public';
  return presentation.kind === 'Remote user'
    ? `Remote user ${presentation.user} at ${presentation.journal}`
    : `Local user ${presentation.user}`;
};
const formatRetrieve = (value: AuthorizationRule['retrieve']) => (
  Array.isArray(value) ? `${value[0]} … ${value[1]}` : value ? 'all indices' : 'disabled'
);
const REMOTE_KEY_INDEX: [number, number] = [-32, -1];
type PrincipalKind = 'empty' | 'local' | 'public' | 'remote' | 'invalid';
const principalKind = (principal: JournalPath): PrincipalKind => {
  if (principal.length === 0) return 'empty';
  if (principal.length === 1 && principal[0] === '*public*') return 'public';
  const terminalLocal = principal.length === 2 && principal[0] === '*state*';
  const terminalRemote = principal.length >= 3 && principal[principal.length - 2] === '*state*';
  if (terminalLocal || terminalRemote) {
    const user = String(principal[principal.length - 1]);
    const route = terminalRemote ? principal.slice(0, -2).map(String) : [];
    const validUser = user !== ''
      && !/\s/.test(user)
      && user !== '*state*'
      && user !== '*public*';
    const validRoute = route.every((segment) => (
      segment !== '' && segment !== '*state*' && segment !== '*public*'
    ));
    if (validUser && validRoute) return terminalRemote ? 'remote' : 'local';
  }
  return 'invalid';
};
const invalidPrincipalMessage = 'Journal location must contain only route aliases, and User must be one local username.';
const parseRange = (startValue: string, endValue: string, label: string): [number, number] => {
  const integer = /^-?\d+$/;
  const startText = startValue.trim();
  const endText = endValue.trim();
  if (!integer.test(startText) || !integer.test(endText)) {
    throw new Error(`${label} requires integer start and end indices.`);
  }
  const start = Number(startText);
  const end = Number(endText);
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end)) {
    throw new Error(`${label} indices must be safe integers.`);
  }
  if (start < 0 && end >= 0) {
    throw new Error(`${label} cannot use a relative start with an absolute end.`);
  }
  if (!(start >= 0 && end < 0) && start > end) {
    throw new Error(`${label} start must not be greater than end.`);
  }
  return [start, end];
};

const emptyRuleForm = {
  principalKind: 'local' as 'local' | 'remote' | 'public',
  journal: '',
  user: '',
  path: '',
  readOnlyUse: false,
  put: false,
  mutatingUse: false,
  run: false,
};

const AccessPanel: React.FC<AccessPanelProps> = ({ journalService, currentUser, refreshKey, isAdmin = false }) => {
  const [rules, setRules] = useState<AuthorizationRule[]>([]);
  const [form, setForm] = useState(emptyRuleForm);
  const [namespaceInput, setNamespaceInput] = useState(currentUser);
  const [activeNamespace, setActiveNamespace] = useState<JournalPath>(['*state*', currentUser]);
  const loadGenerationRef = useRef(0);
  const activeNamespaceRef = useRef<JournalPath>(['*state*', currentUser]);
  const rulesRef = useRef<AuthorizationRule[]>([]);
  const savingRef = useRef(false);
  const confirmationPendingRef = useRef(false);
  const confirmationResolveRef = useRef<((confirmed: boolean) => void) | null>(null);
  const confirmationReturnFocusRef = useRef<HTMLElement | null>(null);
  const [retrieveEnabled, setRetrieveEnabled] = useState(false);
  const [retrieveStart, setRetrieveStart] = useState('0');
  const [retrieveEnd, setRetrieveEnd] = useState('-1');
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [confirmation, setConfirmation] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const managedNamespace = () => activeNamespaceRef.current;

  const requestConfirmation = (message: string): Promise<boolean> => {
    if (confirmationPendingRef.current) return Promise.resolve(false);
    confirmationPendingRef.current = true;
    confirmationReturnFocusRef.current = document.activeElement as HTMLElement | null;
    setConfirmation(message);
    return new Promise((resolve) => { confirmationResolveRef.current = resolve; });
  };

  const answerConfirmation = (confirmed: boolean) => {
    const resolve = confirmationResolveRef.current;
    confirmationResolveRef.current = null;
    confirmationPendingRef.current = false;
    setConfirmation(null);
    resolve?.(confirmed);
    window.setTimeout(() => confirmationReturnFocusRef.current?.focus(), 0);
  };

  const handleConfirmationKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      answerConfirmation(false);
      return;
    }
    if (event.key !== 'Tab') return;
    const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button'));
    if (buttons.length === 0) return;
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.shiftKey
      ? (current <= 0 ? buttons.length - 1 : current - 1)
      : (current >= buttons.length - 1 ? 0 : current + 1);
    event.preventDefault();
    buttons[next].focus();
  };

  const loadRules = async (target: JournalPath, activateTarget: boolean): Promise<boolean> => {
    const generation = ++loadGenerationRef.current;
    setIsLoading(true);
    setError(null);
    try {
      const nextRules = await journalService.getAuthorizations(target);
      if (generation !== loadGenerationRef.current) return false;
      rulesRef.current = nextRules;
      setRules(nextRules);
      if (activateTarget) {
        activeNamespaceRef.current = target;
        setActiveNamespace(target);
      }
      return true;
    } catch (loadError) {
      if (generation !== loadGenerationRef.current) return false;
      setError(loadError instanceof Error ? loadError.message : 'Could not load access rules');
      return false;
    } finally {
      if (generation === loadGenerationRef.current) setIsLoading(false);
    }
  };

  useEffect(() => {
    const target = isAdmin ? activeNamespace : ['*state*', currentUser];
    if (!isAdmin) {
      setNamespaceInput(currentUser);
      activeNamespaceRef.current = target;
      setActiveNamespace(target);
    }
    void loadRules(target, false);
  }, [journalService, currentUser, refreshKey, isAdmin]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleLoadNamespace = () => {
    try {
      const target = parseNamespaceInput(namespaceInput);
      const changed = JSON.stringify(target) !== JSON.stringify(activeNamespace);
      void loadRules(target, changed);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Invalid managed namespace.');
    }
  };

  const buildRule = (): AuthorizationRule => {
    const user = form.user.trim();
    const route = form.principalKind === 'remote' ? splitPathInput(form.journal) : [];
    if (form.principalKind === 'remote' && route.length === 0) {
      throw new Error('Remote user requires a Journal location.');
    }
    const principal: JournalPath = form.principalKind === 'public'
      ? ['*public*']
      : [...route, '*state*', user];
    const kind = principalKind(principal);
    if (kind === 'empty') throw new Error('Principal is required.');
    if (kind === 'invalid') throw new Error(invalidPrincipalMessage);
    const remotePrincipal = kind === 'remote';
    const retrieve = retrieveEnabled
      ? parseRange(retrieveStart, retrieveEnd, 'Retrieve window')
      : false;
    const keyIndex = remotePrincipal ? [...REMOTE_KEY_INDEX] as [number, number] : undefined;
    if (!form.readOnlyUse && !form.put && !form.mutatingUse && !form.run && retrieve === false) {
      throw new Error('Select at least one allowed function.');
    }
    return {
      principal,
      ...(keyIndex ? { 'key-index': keyIndex } : {}),
      path: parseHumanPathInput(form.path),
      'put!': form.put,
      'use!': form.mutatingUse
        ? { 'read-only?': false }
        : form.readOnlyUse ? { 'read-only?': true } : false,
      'run!': form.run,
      retrieve,
    };
  };

  const handleAddRule = async (event: FormEvent) => {
    event.preventDefault();
    if (savingRef.current || confirmationPendingRef.current || isLoading) return;
    savingRef.current = true;
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      const rule = buildRule();
      if (rule.principal.length === 0) throw new Error('Principal is required.');
      const exactRule = JSON.stringify(rule);
      if (rulesRef.current.some((current) => JSON.stringify(current) === exactRule)) {
        throw new Error('Access rule already exists; no change was made.');
      }
      const changed = await journalService.authorize(managedNamespace(), rule);
      if (!changed) throw new Error('Access rule was not added; Journal reported no change.');
      if (!await loadRules(activeNamespaceRef.current, false)
          || !rulesRef.current.some((current) => JSON.stringify(current) === exactRule)) {
        throw new Error('Access rule could not be confirmed in the refreshed rule set.');
      }
      setForm(emptyRuleForm);
      setRetrieveEnabled(false);
      setRetrieveStart('0');
      setRetrieveEnd('-1');
      setStatus('Access rule added.');
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not add access rule');
    } finally {
      savingRef.current = false;
      setIsSaving(false);
    }
  };

  const handleDeleteRule = async (rule: AuthorizationRule) => {
    if (savingRef.current || confirmationPendingRef.current || isLoading) return;
    const generation = loadGenerationRef.current;
    const namespace = JSON.stringify(activeNamespaceRef.current);
    const rawRule = JSON.stringify(rule);
    if (!await requestConfirmation(
      `Remove access rule for ${formatPrincipal(rule.principal)} at ${formatPath(rule.path)}?`,
    )) return;
    if (loadGenerationRef.current !== generation
        || JSON.stringify(activeNamespaceRef.current) !== namespace
        || !rulesRef.current.some((current) => JSON.stringify(current) === rawRule)) {
      setError('Access rules changed during confirmation; review the current rule before deleting it.');
      return;
    }
    savingRef.current = true;
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await journalService.deauthorize(managedNamespace(), rule);
      await loadRules(activeNamespace, false);
      setStatus('Access rule removed.');
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not remove access rule');
    } finally {
      savingRef.current = false;
      setIsSaving(false);
    }
  };

  const mutationLocked = isSaving || confirmation !== null;
  const useEnabled = form.readOnlyUse || form.mutatingUse;

  return (
    <div className="access-panel">
      {confirmation && (
        <div className="access-confirmation-backdrop">
          <div className="access-confirmation" role="dialog" aria-modal="true" aria-labelledby="access-confirmation-title" onKeyDown={handleConfirmationKeyDown}>
            <h2 id="access-confirmation-title">Confirm access-rule deletion</h2>
            <p>{confirmation}</p>
            <div className="access-confirmation-actions">
              <button className="button button-secondary" type="button" onClick={() => answerConfirmation(false)} autoFocus>Cancel</button>
              <button className="button button-primary" type="button" onClick={() => answerConfirmation(true)}>Confirm</button>
            </div>
          </div>
        </div>
      )}
      {(error || status) && (
        <div className={`access-message ${error ? 'error' : 'success'}`}>{error ?? status}</div>
      )}

      <section className="access-card">
        <div className="access-card-header">
          <div>
            <h2>Add rule</h2>
            <p>Rules for <code>{namespaceDisplay(activeNamespace)}</code> independently grant put, use, retrieve, and run operations.</p>
          </div>
        </div>
        <form className="access-form" onSubmit={handleAddRule}>
          {isAdmin && (
            <div className="access-wide-field access-namespace-target">
              <label>
                Manage namespace
                <input value={namespaceInput} onChange={(event) => setNamespaceInput(event.target.value)} placeholder="example: bob" disabled={mutationLocked} />
              </label>
              <button className="button button-secondary" type="button" onClick={handleLoadNamespace} disabled={mutationLocked}>Load namespace</button>
            </div>
          )}
          <label>
            Principal kind
            <select
              value={form.principalKind}
              onChange={(event) => {
                const principalKind = event.target.value as 'local' | 'remote' | 'public';
                setForm({
                  ...form,
                  principalKind,
                  mutatingUse: form.mutatingUse,
                  run: principalKind === 'public' ? false : form.run,
                });
              }}
              disabled={mutationLocked}
            >
              <option value="local">Local user</option>
              <option value="remote">Remote user</option>
              <option value="public">Public</option>
            </select>
          </label>
          {form.principalKind === 'remote' && (
            <label>
              Journal location
              <input value={form.journal} onChange={(event) => setForm({ ...form, journal: event.target.value })} placeholder="example: peer archive" disabled={mutationLocked} />
            </label>
          )}
          {form.principalKind !== 'public' && (
            <label>
              User
              <input value={form.user} onChange={(event) => setForm({ ...form, user: event.target.value })} placeholder="example: alice" disabled={mutationLocked} />
            </label>
          )}
          <label className="access-wide-field">
            Path under your namespace
            <input value={form.path} onChange={(event) => setForm({ ...form, path: event.target.value })} placeholder="example: docs project" disabled={mutationLocked} />
          </label>
          <div className="access-toggles" aria-label="Allowed functions">
            <div className="access-permission-pill">
              <label className="access-switch-row">put <input type="checkbox" checked={form.put} onChange={(event) => setForm({ ...form, put: event.target.checked })} disabled={mutationLocked} /><span className="access-switch" /></label>
            </div>
            <div className={`access-permission-pill ${useEnabled ? 'expanded' : ''}`}>
              <label className="access-switch-row">use <input type="checkbox" checked={useEnabled} onChange={(event) => setForm({ ...form, readOnlyUse: false, mutatingUse: event.target.checked })} disabled={mutationLocked} /><span className="access-switch" /></label>
              {useEnabled && (
                <label className="access-secondary-toggle">read-only <input type="checkbox" checked={form.readOnlyUse} onChange={(event) => setForm({ ...form, readOnlyUse: event.target.checked, mutatingUse: !event.target.checked })} disabled={mutationLocked} /></label>
              )}
            </div>
            <div className={`access-permission-pill ${retrieveEnabled ? 'expanded' : ''}`}>
              <label className="access-switch-row">retrieve <input type="checkbox" checked={retrieveEnabled} onChange={(event) => setRetrieveEnabled(event.target.checked)} disabled={mutationLocked} /><span className="access-switch" /></label>
              {retrieveEnabled && (
                <div className="access-secondary-range">
                  <label>index start <input type="number" value={retrieveStart} onChange={(event) => setRetrieveStart(event.target.value)} disabled={mutationLocked} /></label>
                  <label>index end <input type="number" value={retrieveEnd} onChange={(event) => setRetrieveEnd(event.target.value)} disabled={mutationLocked} /></label>
                </div>
              )}
            </div>
            <div className="access-permission-pill">
              <label className="access-switch-row">run <input type="checkbox" checked={form.run} onChange={(event) => setForm({ ...form, run: event.target.checked })} disabled={mutationLocked || form.principalKind === 'public'} /><span className="access-switch" /></label>
            </div>
          </div>
          <button className="button button-primary access-form-submit" type="submit" disabled={mutationLocked || isLoading}>
            Add
          </button>
        </form>
      </section>

      <section className="access-card">
        <div className="access-card-header">
          <div>
            <h2>Current rules</h2>
            <p>All rules are recursive for <code>{namespaceDisplay(activeNamespace)}</code>.</p>
          </div>

        </div>
        {isLoading && rules.length === 0 ? (
          <div className="access-empty">Loading access rules…</div>
        ) : rules.length === 0 ? (
          <div className="access-empty">No explicit access rules yet. This namespace is private except to its owner and admins.</div>
        ) : (
          <div className="access-rule-list">
            {rules.map((rule, index) => {
              const principal = principalPresentation(rule.principal);
              return (
              <article className="access-rule" key={`${formatPrincipal(rule.principal)}-${formatPath(rule.path)}-${index}`}>
                <div className="access-rule-main">
                  <div className="access-rule-principal">{principal.kind}</div>
                  {principal.kind !== 'Public' && (
                    <div className="access-rule-principal-details">
                      {principal.kind === 'Remote user' && <span>Journal: {principal.journal}</span>}
                      <span>User: {principal.user}</span>
                    </div>
                  )}
                  <div className="access-rule-path">{formatPath(rule.path)}</div>
                </div>
                <div className="access-rule-permissions">
                  {rule['put!'] && <span>put</span>}
                  {rule['use!'] !== false && (
                    <span>{rule['use!']['read-only?'] ? 'use · read-only' : 'use'}</span>
                  )}
                  {rule.retrieve !== false && <span>retrieve · {formatRetrieve(rule.retrieve)}</span>}
                  {rule['run!'] && <span>run</span>}
                </div>
                <button className="button button-primary" type="button" onClick={() => void handleDeleteRule(rule)} disabled={mutationLocked || isLoading}>
                  Delete
                </button>
              </article>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
};

export default AccessPanel;
