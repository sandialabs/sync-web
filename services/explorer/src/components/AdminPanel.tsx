import React, { FormEvent, useEffect, useMemo, useRef, useState } from 'react';
import { JournalService } from '../services/JournalService';
import { AdminBridge, AdminConfig, BridgeDirection, BridgePolicyChoice } from '../types';
import './AdminPanel.css';

interface AdminPanelProps {
  journalService: JournalService;
  currentUser: string;
  refreshKey: number;
}

const emptyConfig: AdminConfig = {
  admins: [],
  bridges: [],
  subscribers: [],
  localName: null,
  windowSize: null,
};

const normalizeName = (value: string) => value.trim();
const validRemoteEndpoint = (value: string) => {
  try {
    const url = new URL(value);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
};
const policyChoices: BridgePolicyChoice[] = ['push', 'pull', 'none'];
const policyLabel = (choice: BridgePolicyChoice) => choice[0].toUpperCase() + choice.slice(1);

const policyAllowed = (bridge: AdminBridge, choice: BridgePolicyChoice): boolean => {
  if (choice === 'none') {
    return true;
  }
  if (bridge.direction === 'incoming') {
    const remotePublish = bridge.remotePolicy.publish;
    if (remotePublish === 'none') return false;
    if (remotePublish === 'push') return choice === 'push' || choice === 'pull';
    if (remotePublish === 'pull') return choice === 'pull';
    return false;
  }
  const remoteSubscribe = bridge.remotePolicy.subscribe;
  if (remoteSubscribe === 'none') return false;
  if (choice === 'push') return remoteSubscribe === 'push' || remoteSubscribe === 'pull';
  if (choice === 'pull') return remoteSubscribe === 'pull';
  return false;
};

const selectedPolicy = (bridge: AdminBridge): BridgePolicyChoice => (
  bridge.direction === 'incoming' ? bridge.localPolicy.subscribe : bridge.localPolicy.publish
);

const withSelectedPolicy = (bridge: AdminBridge, choice: BridgePolicyChoice) => {
  if (bridge.direction === 'incoming') {
    return { ...bridge.localPolicy, subscribe: choice };
  }
  return { ...bridge.localPolicy, publish: choice };
};

const AdminPanel: React.FC<AdminPanelProps> = ({ journalService, currentUser, refreshKey }) => {
  const [config, setConfig] = useState<AdminConfig>(emptyConfig);
  const [adminName, setAdminName] = useState('');
  const [bridgeName, setBridgeName] = useState('');
  const [bridgeEndpoint, setBridgeEndpoint] = useState('');
  const [bridgeDirection, setBridgeDirection] = useState<BridgeDirection>('incoming');
  const [bridgeRemoteName, setBridgeRemoteName] = useState('');
  const [windowInput, setWindowInput] = useState('');
  const windowInputDirtyRef = useRef(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const bridgeEndpointForSharing = `${window.location.origin}/api/v1/journal/interface`;

  const sortedAdmins = useMemo(
    () => [...config.admins].sort((left, right) => left.localeCompare(right)),
    [config.admins],
  );

  const loadConfig = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const nextConfig = await journalService.getAdminConfig();
      setConfig((prev) =>
        JSON.stringify(prev) === JSON.stringify(nextConfig) ? prev : nextConfig,
      );
      const nextWindowInput = nextConfig.windowSize === null ? '' : String(nextConfig.windowSize);
      if (!windowInputDirtyRef.current) {
        setWindowInput((prev) => (prev === nextWindowInput ? prev : nextWindowInput));
      }
      if (nextConfig.localName) {
        setBridgeRemoteName((prev) => prev || nextConfig.localName || '');
      }
      setHasLoaded(true);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : 'Could not load admin config');
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void loadConfig();
  }, [journalService, refreshKey]);

  const saveAdmins = async (admins: string[], message: string) => {
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await journalService.setAdmins(admins);
      setConfig((prev) => ({ ...prev, admins }));
      setStatus(message);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not update admins');
    } finally {
      setIsSaving(false);
    }
  };

  const handleAddAdmin = async (event: FormEvent) => {
    event.preventDefault();
    const name = normalizeName(adminName);
    if (!name) {
      return;
    }
    if (/\s/.test(name)) {
      setError('Admin usernames cannot contain whitespace.');
      return;
    }
    if (sortedAdmins.includes(name)) {
      setError(`${name} is already an admin.`);
      return;
    }

    await saveAdmins([...sortedAdmins, name], `Added ${name} as an admin.`);
    setAdminName('');
  };

  const handleRemoveAdmin = async (name: string) => {
    if (name === currentUser) {
      const confirmed = window.confirm(
        'Remove your own admin access? You may lose access to this tab immediately.',
      );
      if (!confirmed) {
        return;
      }
    }
    await saveAdmins(sortedAdmins.filter((admin) => admin !== name), `Removed ${name} from admins.`);
  };

  const handleSaveBridge = async (event: FormEvent) => {
    event.preventDefault();
    const name = normalizeName(bridgeName);
    const endpoint = bridgeEndpoint.trim();
    if (!name || !endpoint) {
      return;
    }
    if (!validRemoteEndpoint(endpoint)) {
      setError('Remote endpoint must be an http:// or https:// URL.');
      return;
    }
    if (/\s/.test(name)) {
      setError('Bridge names cannot contain whitespace.');
      return;
    }

    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await journalService.saveBridge({
        name,
        endpoint,
        direction: bridgeDirection,
        policy: { publish: 'push', subscribe: 'pull' },
        remoteName: normalizeName(bridgeRemoteName) || config.localName || name,
      });
      await loadConfig();
      setBridgeName('');
      setBridgeEndpoint('');
      setBridgeRemoteName('');
      setStatus(`Saved ${bridgeDirection} bridge ${name}.`);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not save bridge');
    } finally {
      setIsSaving(false);
    }
  };

  const handleSaveWindow = async (event: FormEvent) => {
    event.preventDefault();
    const nextWindow = Number.parseInt(windowInput, 10);
    if (!Number.isInteger(nextWindow) || nextWindow <= 0) {
      setError('Window size must be a positive integer.');
      return;
    }

    if (config.windowSize !== null && nextWindow < config.windowSize) {
      const confirmed = window.confirm(
        `Decrease the window from ${config.windowSize} to ${nextWindow}? Recent unpinned history may be pruned.`,
      );
      if (!confirmed) {
        return;
      }
    }

    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await journalService.setWindowSize(nextWindow);
      setConfig((prev) => ({ ...prev, windowSize: nextWindow }));
      setWindowInput(String(nextWindow));
      windowInputDirtyRef.current = false;
      setStatus(`Updated window size to ${nextWindow}.`);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not update window size');
    } finally {
      setIsSaving(false);
    }
  };

  const saveBridgeUpdate = async (bridge: AdminBridge, input: { endpoint?: string; choice?: BridgePolicyChoice }) => {
    const nextEndpoint = input.endpoint ?? bridge.endpoint;
    const nextPolicy = input.choice ? withSelectedPolicy(bridge, input.choice) : bridge.localPolicy;
    await journalService.saveBridge({
      name: bridge.name,
      endpoint: nextEndpoint,
      direction: bridge.direction,
      policy: nextPolicy,
      remoteName: bridge.remoteName || bridge.name,
    });
  };

  const handleEditBridgeEndpoint = async (bridge: AdminBridge) => {
    const endpoint = window.prompt(`Endpoint for ${bridge.name}`, bridge.endpoint);
    if (endpoint === null) {
      return;
    }
    const trimmed = endpoint.trim();
    if (!trimmed) {
      setError('Bridge endpoint cannot be empty.');
      return;
    }
    if (!validRemoteEndpoint(trimmed)) {
      setError('Remote endpoint must be an http:// or https:// URL.');
      return;
    }
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await saveBridgeUpdate(bridge, { endpoint: trimmed });
      await loadConfig();
      setStatus(`Updated ${bridge.name} endpoint.`);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not update bridge endpoint');
    } finally {
      setIsSaving(false);
    }
  };

  const handleSetBridgePolicy = async (bridge: AdminBridge, choice: BridgePolicyChoice) => {
    if (!policyAllowed(bridge, choice) || selectedPolicy(bridge) === choice) {
      return;
    }
    if (choice === 'none' && !window.confirm(`Set ${bridge.name} to none? This will sever synchronization but keep the entry in config.`)) {
      return;
    }
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await saveBridgeUpdate(bridge, { choice });
      await loadConfig();
      setStatus(`Updated ${bridge.name} policy to ${choice}.`);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not update bridge policy');
    } finally {
      setIsSaving(false);
    }
  };

  const handleDeleteBridge = async (bridge: AdminBridge) => {
    if (!window.confirm(`Delete ${bridge.name}? This removes the config entry rather than keeping it disabled.`)) {
      return;
    }
    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await journalService.deleteBridge(bridge.name, bridge.direction);
      await loadConfig();
      setStatus(`Deleted ${bridge.name}.`);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not delete bridge');
    } finally {
      setIsSaving(false);
    }
  };

  const renderBridgeList = (title: string, bridges: AdminBridge[], empty: string) => (
    <section className="admin-subsection">
      <div className="admin-subsection-label">{title}</div>
      <div className="admin-list bridge-list">
        {bridges.length === 0 ? (
          <div className="admin-empty">{empty}</div>
        ) : (
          bridges.map((bridge) => (
            <div className="admin-row bridge-row" key={`${bridge.direction}-${bridge.name}`}>
              <span className="bridge-name">{bridge.name}</span>
              <span className="bridge-endpoint">{bridge.endpoint || 'Endpoint unavailable'}</span>
              <span className="bridge-policy-toggle" role="radiogroup" aria-label={`${bridge.name} policy`}>
                {policyChoices.map((choice) => {
                  const active = selectedPolicy(bridge) === choice;
                  const allowed = policyAllowed(bridge, choice);
                  return (
                    <button
                      key={choice}
                      type="button"
                      role="radio"
                      aria-checked={active}
                      className={`segmented-option ${active ? 'active' : ''}`}
                      disabled={isSaving || !allowed}
                      onClick={() => void handleSetBridgePolicy(bridge, choice)}
                      title={!allowed ? 'Remote policy does not allow this option' : undefined}
                    >
                      {policyLabel(choice)}
                    </button>
                  );
                })}
              </span>
              <button
                type="button"
                className="icon-button"
                aria-label={`Edit ${bridge.name} endpoint`}
                title="Edit endpoint"
                disabled={isSaving}
                onClick={() => void handleEditBridgeEndpoint(bridge)}
              >
                ✎
              </button>
              <button
                type="button"
                className="icon-button danger"
                aria-label={`Delete ${bridge.name}`}
                title="Delete bridge"
                disabled={isSaving}
                onClick={() => void handleDeleteBridge(bridge)}
              >
                🗑
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );

  const handleCopyLocalEndpoint = async () => {
    setError(null);
    setStatus(null);
    try {
      await navigator.clipboard.writeText(bridgeEndpointForSharing);
      setStatus('Copied local bridge endpoint.');
    } catch {
      setError('Could not copy endpoint.');
    }
  };

  if (isLoading && !hasLoaded) {
    return (
      <div className="admin-panel">
        <div className="admin-loading">Loading admin controls...</div>
      </div>
    );
  }

  return (
    <div className="admin-panel">
      {(error || status) && (
        <div className={`admin-message ${error ? 'error' : 'success'}`}>
          {error ?? status}
        </div>
      )}

      <section className="admin-section">
        <div className="admin-section-header">
          <h2>Bridges</h2>
        </div>
        <div className="local-endpoint-block">
          <div className="local-endpoint-label">Local endpoint</div>
          <div className="local-endpoint-row">
            <code>{bridgeEndpointForSharing}</code>
            <button className="icon-button" onClick={handleCopyLocalEndpoint} aria-label="Copy local endpoint" title="Copy local endpoint">
              ⧉
            </button>
          </div>
        </div>
        {renderBridgeList('Subscribe to', config.bridges, 'No subscribed bridges configured.')}
        {renderBridgeList('Publish to', config.subscribers, 'No publishing targets configured.')}
        <form className="admin-form bridge-form" onSubmit={handleSaveBridge}>
          <select
            className="input"
            value={bridgeDirection}
            onChange={(event) => {
              const direction = event.target.value as BridgeDirection;
              setBridgeDirection(direction);
              if (direction === 'outgoing' && config.localName) {
                setBridgeRemoteName((prev) => prev || config.localName || '');
              }
            }}
            disabled={isSaving}
          >
            <option value="incoming">Subscribe to</option>
            <option value="outgoing">Publish to</option>
          </select>
          <input
            className="input"
            value={bridgeName}
            onChange={(event) => setBridgeName(event.target.value)}
            placeholder="Bridge name"
            disabled={isSaving}
          />
          <input
            className="input"
            value={bridgeEndpoint}
            onChange={(event) => setBridgeEndpoint(event.target.value)}
            placeholder="Remote endpoint"
            disabled={isSaving}
          />
          <input
            className="input"
            value={bridgeRemoteName}
            onChange={(event) => setBridgeRemoteName(event.target.value)}
            placeholder="Remote name"
            disabled={isSaving}
          />
          <button
            className="button button-primary"
            type="submit"
            disabled={isSaving || !bridgeName.trim() || !bridgeEndpoint.trim()}
          >
            Create bridge
          </button>
        </form>
      </section>

      <section className="admin-section advanced">
        <div className="admin-section-header">
          <h2>Window Size</h2>
        </div>
        <form className="admin-form inline" onSubmit={handleSaveWindow}>
          <input
            className="input numeric"
            type="number"
            min="1"
            step="1"
            value={windowInput}
            onChange={(event) => {
              setWindowInput(event.target.value);
              windowInputDirtyRef.current = true;
            }}
            disabled={isSaving}
          />
          <button className="button button-primary" type="submit" disabled={isSaving || !windowInput.trim()}>
            Update Window
          </button>
        </form>
      </section>

      <section className="admin-section">
        <div className="admin-section-header">
          <h2>Admin Users</h2>
        </div>
        <div className="admin-list">
          {sortedAdmins.length === 0 ? (
            <div className="admin-empty">No admins configured.</div>
          ) : (
            sortedAdmins.map((name) => (
              <div className="admin-row" key={name}>
                <span>{name}</span>
                <button
                  className="button button-secondary"
                  onClick={() => handleRemoveAdmin(name)}
                  disabled={isSaving}
                >
                  Remove
                </button>
              </div>
            ))
          )}
        </div>
        <form className="admin-form inline" onSubmit={handleAddAdmin}>
          <input
            className="input"
            value={adminName}
            onChange={(event) => setAdminName(event.target.value)}
            placeholder="Username"
            disabled={isSaving}
          />
          <button className="button button-primary" type="submit" disabled={isSaving || !adminName.trim()}>
            Add Admin
          </button>
        </form>
      </section>
    </div>
  );
};

export default AdminPanel;
