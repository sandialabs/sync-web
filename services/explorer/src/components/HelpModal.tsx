import React from 'react';
import './HelpModal.css';

interface HelpModalProps {
  onClose: () => void;
}

const HelpModal: React.FC<HelpModalProps> = ({ onClose }) => {
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Synchronic Web Explorer Help</h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>
        <div className="modal-body">
          <section>
            <h3>Overview</h3>
            <p>
              The Synchronic Web Explorer has Ledger and Stage modes for ordinary use.
              Interface admins also see Admin controls for bridge and journal settings.
            </p>
          </section>

          <section>
            <h3>Getting Started</h3>
            <ol>
              <li>Use Stage to browse and edit documents and folders.</li>
              <li>Use Ledger to synchronize and browse committed state.</li>
              <li>Clicking either tab returns that mode to the Self namespace root.</li>
              <li>Use Admin to manage bridges, window size, and interface admins. Destructive bridge, preapproval, and retention-window actions require confirmation.</li>
            </ol>
          </section>

          <section>
            <h3>Stage Mode</h3>
            <ul>
              <li>The permanent <code>*state*</code> row returns to the browsing-only current namespace root.</li>
              <li>The left tree shows staged documents and folders beneath that root.</li>
              <li>Remote descendant selections optimistically show mutation controls; the terminal journal decides authorization and denied edits remain available for correction.</li>
              <li>Tree rows provide rename and delete actions.</li>
              <li>Selecting a directory shows its contents and document/folder creation actions.</li>
              <li>Selecting a document shows a read-only view until you press Edit.</li>
            </ul>
          </section>

          <section>
            <h3>Ledger Mode</h3>
            <ul>
              <li>The route strip defines the current committed view.</li>
              <li>Click a breadcrumb to return both trees to that journal's namespace root.</li>
              <li>The first hop is the local journal and additional hops extend through bridges.</li>
              <li>Each hop accepts <code>latest</code> or a negative snapshot index.</li>
              <li>The tree below the route strip shows the state at the current route tip.</li>
            </ul>
          </section>

          <section>
            <h3>Access Mode</h3>
            <ul>
              <li>A complete remote principal has one or more route segments followed by <code>*state* USER</code>.</li>
              <li>Partial or malformed principal text reports the accepted shapes on submission.</li>
              <li>The Document history window governs which committed indexes resolve may access.</li>
              <li>Quote owner-relative path segments containing spaces; percent, quote, and backslash characters are encoded once and decoded for display.</li>
            </ul>
          </section>

          <section>
            <h3>Content Pane</h3>
            <ul>
              <li>Directories are shown as a simple contents view.</li>
              <li>Documents can be viewed as content in both modes.</li>
              <li>In Ledger, the content header toggles between content and proof for the current document.</li>
              <li>Pinning is available from the ledger document header.</li>
            </ul>
          </section>
        </div>
      </div>
    </div>
  );
};

export default HelpModal;
