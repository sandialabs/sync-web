import React, { useEffect, useState } from 'react';
import { LedgerHop } from '../types';
import {
  normalizePublicSnapshotInput, normalizeSnapshotInput,
} from '../utils/ledgerRoute';
import { decodeSafeName } from '../utils/nameCodec';

interface SnapshotInputProps {
  hop: LedgerHop;
  index: number;
  displayValue: string;
  normalize: (rawValue: string) => string | null;
  onCommit: (index: number, value: string) => void;
}

const SnapshotInput: React.FC<SnapshotInputProps> = ({
  hop,
  index,
  displayValue,
  normalize,
  onCommit,
}) => {
  const [draft, setDraft] = useState(displayValue);
  const [editing, setEditing] = useState(false);

  useEffect(() => {
    if (!editing) setDraft(displayValue);
  }, [displayValue, editing]);

  const commit = () => {
    const value = normalize(draft);
    setEditing(false);
    if (value === null) {
      setDraft(displayValue);
      return;
    }
    setDraft(value);
    onCommit(index, value);
  };

  return (
    <input
      value={draft}
      onFocus={() => setEditing(true)}
      onChange={(event) => {
        setEditing(true);
        setDraft(event.target.value);
      }}
      onBlur={() => editing && commit()}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault();
          commit();
        } else if (event.key === 'Escape') {
          event.preventDefault();
          setDraft(displayValue);
          setEditing(false);
        }
      }}
      aria-label={`${hop.kind === 'bridge' ? decodeSafeName(hop.name) : hop.name} snapshot`}
    />
  );
};

interface LedgerRouteBarProps {
  hops: LedgerHop[];
  peerChoices?: string[] | null;
  rootIndex: number;
  onSynchronize: () => void;
  isSynchronizing?: boolean;
  onSnapshotChange: (index: number, value: string) => void;
  onStepSnapshot: (index: number, direction: 'older' | 'newer') => void;
  onRemoveHop?: () => void;
  onSelectHop?: (index: number) => void;
  onOpenPeerPicker?: () => void;
  onClosePeerPicker?: () => void;
  onChoosePeer?: (peerName: string) => void;
  readOnlyRoute?: boolean;
}

const LedgerRouteBar: React.FC<LedgerRouteBarProps> = ({
  hops,
  peerChoices = null,
  rootIndex,
  onSynchronize,
  isSynchronizing = false,
  onSnapshotChange,
  onStepSnapshot,
  onRemoveHop = () => undefined,
  onSelectHop = () => undefined,
  onOpenPeerPicker = () => undefined,
  onClosePeerPicker = () => undefined,
  onChoosePeer = () => undefined,
  readOnlyRoute = false,
}) => {
  const pickerOpen = Array.isArray(peerChoices);
  const maximumFor = (hop: LedgerHop, index: number): number =>
    hop.maximum ?? (index === 0 ? Math.max(0, rootIndex) : Math.max(0, Number.parseInt(hop.snapshot, 10) || 0));

  const getSnapshotDisplayValue = (hop: LedgerHop, index: number): string =>
    normalizeSnapshotInput(hop.snapshot, maximumFor(hop, index));

  const normalizeHopInput = (hop: LedgerHop, index: number, rawValue: string): string | null =>
    normalizePublicSnapshotInput(rawValue, maximumFor(hop, index));

  return (
    <div className="route-builder unified">
      {hops.map((hop, index) => (
        <React.Fragment key={hop.key}>
          {index > 0 && <span className="arrow big">→</span>}
          <div
            className={`hop-card-unified ${index === hops.length - 1 ? 'active-hop' : ''} ${
              index === 0 ? 'root-hop-card' : ''
            }`}
          >
            {index === 0 && (
              <button
                className={`sync-pill sync-pill-inline ${isSynchronizing ? 'synchronizing' : ''}`}
                title="Synchronize latest committed root"
                onClick={onSynchronize}
                disabled={isSynchronizing}
              >
                <span className="sync-pill-icon" aria-hidden="true">⟳</span>
              </button>
            )}
            <button
              className="hop-tag route-hop-button"
              onClick={() => onSelectHop(index)}
              aria-current={index === hops.length - 1 ? 'location' : undefined}
            >
              {hop.kind === 'bridge' ? decodeSafeName(hop.name) : hop.name}
            </button>
            <div className="stepper linear">
              <button
                aria-label={`Older ${hop.kind === 'bridge' ? decodeSafeName(hop.name) : hop.name} snapshot`}
                onClick={() => onStepSnapshot(index, 'older')}
                disabled={Number.parseInt(getSnapshotDisplayValue(hop, index), 10) <= 0}
              >-</button>
              <SnapshotInput
                hop={hop}
                index={index}
                displayValue={getSnapshotDisplayValue(hop, index)}
                normalize={(value) => normalizeHopInput(hop, index, value)}
                onCommit={onSnapshotChange}
              />
              <button
                aria-label={`Newer ${hop.kind === 'bridge' ? decodeSafeName(hop.name) : hop.name} snapshot`}
                onClick={() => onStepSnapshot(index, 'newer')}
                disabled={Number.parseInt(getSnapshotDisplayValue(hop, index), 10) >= maximumFor(hop, index)}
              >+</button>
            </div>
          </div>
        </React.Fragment>
      ))}

      {!readOnlyRoute && <div className="route-control-rail">
        {pickerOpen ? (
          <div className="peer-picker-shell">
            <button className="route-action" title="Exit bridge selection" onClick={onClosePeerPicker}>
              ×
            </button>
            <div className="peer-rail">
              <div className="peer-rail-scroll">
                {peerChoices.length > 0 ? (
                  peerChoices.map((peerName) => (
                    <button
                      key={peerName}
                      className="peer-pill"
                      onClick={() => onChoosePeer(peerName)}
                    >
                      {peerName}
                    </button>
                  ))
                ) : (
                  <div className="peer-rail-empty">No bridges available</div>
                )}
              </div>
            </div>
          </div>
        ) : (
          <>
            <button
              className="route-action"
              title="Move back one journal"
              onClick={onRemoveHop}
              disabled={hops.length <= 1}
            >
              ←
            </button>
            <button className="route-action ghost" title="Open a bridge" onClick={onOpenPeerPicker}>
              →
            </button>
          </>
        )}
      </div>}
    </div>
  );
};

export default LedgerRouteBar;
