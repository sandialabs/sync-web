import React, { FormEvent, useEffect, useMemo, useRef, useState } from 'react';
import { JournalService } from '../services/JournalService';
import { AdminConfig } from '../types';
import './AdminPanel.css';

interface AdminPanelProps {
  journalService: JournalService;
  currentUser: string;
  refreshKey: number;
}

const emptyConfig: AdminConfig = {
  admins: [],
  bridges: [],
  localName: null,
  localEndpoint: null,
  windowSize: null,
  bridgeAccept: 'auto',
  bridgePreapprovals: {},
};

const validEndpoint = (value: string) => {
  try {
    const url = new URL(value);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
};

const AdminPanel: React.FC<AdminPanelProps> = ({ journalService, currentUser, refreshKey }) => {
  const [config, setConfig] = useState<AdminConfig>(emptyConfig);
  const [adminName, setAdminName] = useState('');
  const [bridgeName, setBridgeName] = useState('');
  const [bridgeEndpoint, setBridgeEndpoint] = useState('');
  const [bridgeRemoteName, setBridgeRemoteName] = useState('');
  const [preapprovalName, setPreapprovalName] = useState('');
  const [preapprovalKey, setPreapprovalKey] = useState('');
  const [windowInput, setWindowInput] = useState('');
  const windowInputDirtyRef = useRef(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const localEndpoint = config.localEndpoint || `${window.location.origin}/api/v1/journal/interface`;

  const sortedAdmins = useMemo(
    () => [...config.admins].sort((left, right) => left.localeCompare(right)),
    [config.admins],
  );

  const load = async () => {
    setIsLoading(true);
    try {
      const next = await journalService.getAdminConfig();
      setConfig(next);
      if (!windowInputDirtyRef.current) setWindowInput(next.windowSize?.toString() ?? '');
      setBridgeRemoteName((value) => value || next.localName || '');
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Unable to load administration state.');
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => { void load(); }, [journalService, refreshKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const run = async (operation: () => Promise<unknown>, message: string) => {
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await operation();
      setStatus(message);
      await load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Administration request failed.');
    } finally {
      setIsSaving(false);
    }
  };

  const createBridge = async (event: FormEvent) => {
    event.preventDefault();
    const name = bridgeName.trim();
    const endpoint = bridgeEndpoint.trim();
    const remoteName = bridgeRemoteName.trim() || config.localName || name;
    if (!name || /\s/.test(name)) return setError('Bridge names cannot contain whitespace.');
    if (!validEndpoint(endpoint)) return setError('Peer endpoint must be an HTTP or HTTPS URL.');
    await run(
      () => journalService.saveBridge({ name, endpoint, remoteName }),
      `Created bridge ${name}.`,
    );
    setBridgeName('');
    setBridgeEndpoint('');
  };

  const setAcceptance = async (mode: 'auto' | 'preapproved') => {
    await run(
      () => journalService.updateConfig(['public', 'bridge-accept'], mode),
      `Incoming bridge acceptance set to ${mode}.`,
    );
  };

  const addPreapproval = async (event: FormEvent) => {
    event.preventDefault();
    const name = preapprovalName.trim();
    const key = preapprovalKey.trim();
    if (!name || !/^[0-9a-f]{64}$/i.test(key)) {
      return setError('Preapproval requires a bridge name and a 32-byte hexadecimal journal ID.');
    }
    await run(
      () => journalService.updateConfig(
        ['private', 'bridge-preapproval', name],
        { '*type/byte-vector*': key },
      ),
      `Preapproved ${name}.`,
    );
    setPreapprovalName('');
    setPreapprovalKey('');
  };

  const removePreapproval = async (name: string) => {
    await run(
      () => journalService.updateConfig(['private', 'bridge-preapproval', name], []),
      `Removed preapproval for ${name}.`,
    );
  };

  const preapprovalKeyText = (value: unknown): string => {
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      const hex = (value as Record<string, unknown>)['*type/byte-vector*'];
      if (typeof hex === 'string') return hex;
    }
    return 'configured';
  };

  const saveWindow = async (event: FormEvent) => {
    event.preventDefault();
    const value = Number.parseInt(windowInput, 10);
    if (!Number.isInteger(value) || value <= 0) return setError('Window size must be positive.');
    await run(() => journalService.setWindowSize(value), 'Updated window size.');
    windowInputDirtyRef.current = false;
  };

  const addAdmin = async (event: FormEvent) => {
    event.preventDefault();
    const name = adminName.trim();
    if (!name) return;
    await run(
      () => journalService.setAdmins(Array.from(new Set([...config.admins, name]))),
      `Added administrator ${name}.`,
    );
    setAdminName('');
  };

  return (
    <div className="admin-panel">
      {(error || status) && <div className={`admin-message ${error ? 'error' : 'success'}`}>{error ?? status}</div>}

      <section className="admin-section bridges-section">
        <div className="admin-section-header"><h2>Bridges</h2></div>
        <details className="local-endpoint-block">
          <summary>This journal&apos;s bridge endpoint</summary>
          <div className="local-endpoint-row">
            <code>{localEndpoint}</code>
            <button className="icon-button" onClick={() => void navigator.clipboard.writeText(localEndpoint)} aria-label="Copy bridge endpoint">⧉</button>
          </div>
        </details>
        <div className="bridge-list">
          {config.bridges.length === 0 ? <div className="admin-empty">No bridges configured.</div> : config.bridges.map((bridge) => (
            <article className="bridge-card" key={bridge.name}>
              <div className="bridge-card-header">
                <strong>{bridge.name}</strong>
                <span className="bridge-role">{bridge.initiation === 'local' ? 'Synchronizes here' : 'Synchronizes at peer'}</span>
              </div>
              <dl className="bridge-details">
                <div><dt>Endpoint</dt><dd><code>{bridge.endpoint}</code></dd></div>
                <div><dt>Name at peer</dt><dd>{bridge.remoteName || 'Not reported'}</dd></div>
                {(bridge.lastIndex !== undefined || bridge.remoteIndex !== undefined) && (
                  <div><dt>Progress</dt><dd>
                    {bridge.lastIndex !== undefined ? `Received ${bridge.lastIndex}` : 'Not received'}
                    {bridge.remoteIndex !== undefined ? ` · acknowledged ${bridge.remoteIndex}` : ''}
                  </dd></div>
                )}
              </dl>
              <button className="button button-secondary bridge-delete" disabled={isSaving} onClick={() => void run(
                () => journalService.deleteBridge(bridge.name), `Deleted bridge ${bridge.name}.`,
              )}>Delete</button>
            </article>
          ))}
        </div>
        <form className="admin-form bridge-create-form" onSubmit={createBridge}>
          <h3>Add bridge</h3>
          <label><span>Name here</span><input className="input" value={bridgeName} onChange={(event) => setBridgeName(event.target.value)} placeholder="peer" disabled={isSaving} /></label>
          <label><span>Peer endpoint</span><input className="input" value={bridgeEndpoint} onChange={(event) => setBridgeEndpoint(event.target.value)} placeholder="https://peer.example/api/v1/journal/interface" disabled={isSaving} /></label>
          <label><span>Name at peer</span><input className="input" value={bridgeRemoteName} onChange={(event) => setBridgeRemoteName(event.target.value)} placeholder="Name peer uses for this journal" disabled={isSaving} /></label>
          <button className="button button-primary" type="submit" disabled={isSaving || !bridgeName.trim() || !bridgeEndpoint.trim()}>Add bridge</button>
        </form>
      </section>

      <section className="admin-section advanced">
        <div className="admin-section-header"><h2>Incoming Bridge Requests</h2></div>
        <p className="admin-section-description">Choose whether unknown journals can create a bridge or must be allowed here first.</p>
        <div className="acceptance-options">
          <label><input type="radio" checked={config.bridgeAccept === 'auto'} onChange={() => void setAcceptance('auto')} /> <span><strong>Automatically accept</strong><small>Allow any journal with a valid signed head.</small></span></label>
          <label><input type="radio" checked={config.bridgeAccept === 'preapproved'} onChange={() => void setAcceptance('preapproved')} /> <span><strong>Require preapproval</strong><small>Only allow the names and journal IDs listed below.</small></span></label>
        </div>
        {config.bridgeAccept === 'preapproved' && (
          <div className="preapproval-panel">
            <form className="admin-form preapproval-form" onSubmit={addPreapproval}>
              <h3>Allow an incoming bridge</h3>
              <label><span>Bridge name</span><input className="input" value={preapprovalName} onChange={(event) => setPreapprovalName(event.target.value)} placeholder="peer" /></label>
              <label><span>Journal ID</span><input className="input" value={preapprovalKey} onChange={(event) => setPreapprovalKey(event.target.value)} placeholder="64 hexadecimal characters" /></label>
              <button className="button button-primary" type="submit" disabled={isSaving}>Allow bridge</button>
            </form>
            <div className="preapproval-list">
              <h3>Allowed bridges</h3>
              {Object.keys(config.bridgePreapprovals).length === 0 ? <div className="admin-empty">No incoming bridges are allowed yet.</div> : Object.keys(config.bridgePreapprovals).sort().map((name) => (
                <div className="preapproval-row" key={name}>
                  <strong>{name}</strong>
                  <code title={preapprovalKeyText(config.bridgePreapprovals[name])}>{preapprovalKeyText(config.bridgePreapprovals[name])}</code>
                  <button className="button button-secondary" disabled={isSaving} onClick={() => void removePreapproval(name)}>Remove</button>
                </div>
              ))}
            </div>
          </div>
        )}
      </section>

      <section className="admin-section advanced">
        <div className="admin-section-header"><h2>Window Size</h2></div>
        <form className="admin-form inline" onSubmit={saveWindow}>
          <input className="input numeric" type="number" min="1" value={windowInput} onChange={(event) => { setWindowInput(event.target.value); windowInputDirtyRef.current = true; }} />
          <button className="button button-primary" type="submit" disabled={isSaving}>Update Window</button>
        </form>
      </section>

      <section className="admin-section">
        <div className="admin-section-header"><h2>Admin Users</h2></div>
        <div className="admin-list">
          {sortedAdmins.map((name) => (
            <div className="admin-row" key={name}><span>{name}</span><button className="button button-secondary" disabled={isSaving || name === currentUser} onClick={() => void run(
              () => journalService.setAdmins(config.admins.filter((admin) => admin !== name)), `Removed administrator ${name}.`,
            )}>Remove</button></div>
          ))}
        </div>
        <form className="admin-form inline" onSubmit={addAdmin}>
          <input className="input" value={adminName} onChange={(event) => setAdminName(event.target.value)} placeholder="Username" />
          <button className="button button-primary" type="submit" disabled={isSaving || !adminName.trim()}>Add Admin</button>
        </form>
      </section>
      {isLoading && <div className="admin-loading">Loading…</div>}
    </div>
  );
};

export default AdminPanel;
