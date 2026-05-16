import React, { FormEvent, useEffect, useMemo, useState } from 'react';
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
  windowSize: null,
};

const normalizeName = (value: string) => value.trim();

const AdminPanel: React.FC<AdminPanelProps> = ({ journalService, currentUser, refreshKey }) => {
  const [config, setConfig] = useState<AdminConfig>(emptyConfig);
  const [adminName, setAdminName] = useState('');
  const [bridgeName, setBridgeName] = useState('');
  const [bridgeEndpoint, setBridgeEndpoint] = useState('');
  const [windowInput, setWindowInput] = useState('');
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
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
      setConfig(nextConfig);
      setWindowInput(nextConfig.windowSize === null ? '' : String(nextConfig.windowSize));
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
    if (/\s/.test(name)) {
      setError('Bridge names cannot contain whitespace.');
      return;
    }

    setIsSaving(true);
    setError(null);
    setStatus(null);
    try {
      await journalService.addBridge(name, endpoint);
      await loadConfig();
      setBridgeName('');
      setBridgeEndpoint('');
      setStatus(`Saved bridge ${name}.`);
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
      setStatus(`Updated window size to ${nextWindow}.`);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : 'Could not update window size');
    } finally {
      setIsSaving(false);
    }
  };

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

  if (isLoading) {
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
            <button className="button button-secondary" onClick={handleCopyLocalEndpoint}>
              Copy
            </button>
          </div>
        </div>
        <div className="admin-list bridge-list">
          {config.bridges.length === 0 ? (
            <div className="admin-empty">No bridges configured.</div>
          ) : (
            config.bridges.map((bridge) => (
              <div className="admin-row bridge-row" key={bridge.name}>
                <span className="bridge-name">{bridge.name}</span>
                <span className="bridge-endpoint">{bridge.endpoint || 'Endpoint unavailable'}</span>
              </div>
            ))
          )}
        </div>
        <form className="admin-form bridge-form" onSubmit={handleSaveBridge}>
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
            placeholder="http://peer-router/api/v1/journal/interface"
            disabled={isSaving}
          />
          <button
            className="button button-primary"
            type="submit"
            disabled={isSaving || !bridgeName.trim() || !bridgeEndpoint.trim()}
          >
            Save Bridge
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
            onChange={(event) => setWindowInput(event.target.value)}
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
