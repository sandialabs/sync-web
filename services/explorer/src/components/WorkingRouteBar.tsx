import React from 'react';

interface WorkingRouteBarProps {
  route: string[];
  peerChoices: string[] | null;
  onRemoveHop: () => void;
  onSelectHop: (index: number) => void;
  onOpenPeerPicker: () => void;
  onClosePeerPicker: () => void;
  onChoosePeer: (peerName: string) => void;
}

const WorkingRouteBar: React.FC<WorkingRouteBarProps> = ({
  route,
  peerChoices,
  onRemoveHop,
  onSelectHop,
  onOpenPeerPicker,
  onClosePeerPicker,
  onChoosePeer,
}) => (
  <div className="route-builder unified working-route" aria-label="Working journal route">
    {['Self', ...route].map((name, index, names) => (
      <React.Fragment key={`${name}-${index}`}>
        {index > 0 && <span className="arrow big">→</span>}
        <div className={`hop-card-unified ${index === names.length - 1 ? 'active-hop' : ''}`}>
          <button
            className="hop-tag route-hop-button"
            onClick={() => onSelectHop(index)}
            aria-current={index === names.length - 1 ? 'location' : undefined}
          >
            {name}
          </button>
        </div>
      </React.Fragment>
    ))}
    <div className="route-control-rail">
      {peerChoices ? (
        <div className="peer-picker-shell">
          <button className="route-action" onClick={onClosePeerPicker}>×</button>
          <div className="peer-rail"><div className="peer-rail-scroll">
            {peerChoices.length > 0 ? peerChoices.map((name) => (
              <button key={name} className="peer-pill" onClick={() => onChoosePeer(name)}>{name}</button>
            )) : <div className="peer-rail-empty">No bridges available</div>}
          </div></div>
        </div>
      ) : (
        <>
          <button className="route-action" title="Move back one journal" onClick={onRemoveHop} disabled={route.length === 0}>←</button>
          <button className="route-action ghost" title="Open a bridge" onClick={onOpenPeerPicker}>→</button>
        </>
      )}
    </div>
  </div>
);

export default WorkingRouteBar;
