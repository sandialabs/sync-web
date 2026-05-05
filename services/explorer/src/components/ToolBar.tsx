import React, { useState } from 'react';
import './ToolBar.css';
import HelpModal from './HelpModal';
import { ExplorerMode } from '../types';

interface ToolBarProps {
  email: string;
  error: string | null;
  isLoading: boolean;
  mode: ExplorerMode;
  theme: 'light' | 'dark';
  onModeChange: (mode: ExplorerMode) => void;
  onThemeToggle: () => void;
}

const ToolBar: React.FC<ToolBarProps> = ({
  email,
  error,
  isLoading,
  mode,
  theme,
  onModeChange,
  onThemeToggle,
}) => {
  const [showHelp, setShowHelp] = useState(false);

  const handleLogout = async () => {
    const returnTo = encodeURIComponent(window.location.href);
    const res = await fetch(
      `/auth/.ory/self-service/logout/browser?return_to=${returnTo}`,
    );
    if (res.ok) {
      const data = await res.json();
      if (data.logout_url) {
        window.location.href = data.logout_url;
      }
    }
  };

  const handleLogoClick = () => {
    window.location.href = window.location.pathname + window.location.search;
  };

  return (
    <>
      <div className="toolbar">
        <div className="toolbar-left">
          <img
            src={process.env.PUBLIC_URL + '/logo.png'}
            alt="Synchronic Web"
            className="toolbar-logo"
            onClick={handleLogoClick}
            title="Return to home"
          />
          <div className="mode-switch">
            <button
              className={`tab ${mode === 'ledger' ? 'active' : ''}`}
              onClick={() => onModeChange('ledger')}
            >
              Ledger
            </button>
            <button
              className={`tab ${mode === 'stage' ? 'active' : ''}`}
              onClick={() => onModeChange('stage')}
            >
              Stage
            </button>
          </div>
        </div>

        <div className="toolbar-right">
          {email && (
            <span className="toolbar-session">
              <span className="toolbar-email" title={email}>{email}</span>
              <button className="button toolbar-logout" onClick={handleLogout}>
                Sign out
              </button>
            </span>
          )}
          <button
            className="button button-icon"
            onClick={onThemeToggle}
            title={theme === 'light' ? 'Switch to dark mode' : 'Switch to light mode'}
          >
            {theme === 'light' ? '◐' : '◑'}
          </button>
          <button
            className="button button-icon"
            onClick={() => setShowHelp(true)}
            title="Help"
            aria-label="Help"
          >
            ⓘ
          </button>
        </div>
      </div>

      {(error || isLoading) && (
        <div className={`toolbar-status-line ${error ? 'error' : ''}`}>
          {error ?? 'Loading...'}
        </div>
      )}

      {showHelp && <HelpModal onClose={() => setShowHelp(false)} />}
    </>
  );
};

export default ToolBar;
