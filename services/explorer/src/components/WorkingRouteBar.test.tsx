import React from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import WorkingRouteBar from './WorkingRouteBar';

describe('WorkingRouteBar', () => {
  it('renders the active route and moves back one hop', () => {
    const onRemoveHop = jest.fn();
    render(
      <WorkingRouteBar
        route={['peer']}
        peerChoices={null}
        onRemoveHop={onRemoveHop}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={jest.fn()}
      />,
    );

    expect(screen.getByText('peer')).toBeInTheDocument();
    fireEvent.click(screen.getByTitle('Move back one journal'));
    expect(onRemoveHop).toHaveBeenCalledTimes(1);
  });

  it('disables moving back at Self', () => {
    render(
      <WorkingRouteBar
        route={[]}
        peerChoices={null}
        onRemoveHop={jest.fn()}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={jest.fn()}
      />,
    );

    expect(screen.getByTitle('Move back one journal')).toBeDisabled();
  });
});
