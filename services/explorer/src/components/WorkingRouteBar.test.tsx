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
        onSelectHop={jest.fn()}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={jest.fn()}
      />,
    );

    expect(screen.getByText('peer')).toBeInTheDocument();
    fireEvent.click(screen.getByTitle('Move back one journal'));
    expect(onRemoveHop).toHaveBeenCalledTimes(1);
  });

  it('makes every breadcrumb hop clickable by prefix index', () => {
    const onSelectHop = jest.fn();
    render(
      <WorkingRouteBar
        route={['peer', 'terminal']}
        peerChoices={null}
        onRemoveHop={jest.fn()}
        onSelectHop={onSelectHop}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={jest.fn()}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Self' }));
    fireEvent.click(screen.getByRole('button', { name: 'peer' }));
    fireEvent.click(screen.getByRole('button', { name: 'terminal' }));
    expect(onSelectHop.mock.calls).toEqual([[0], [1], [2]]);
  });

  it('decodes names for display while preserving exact route aliases', () => {
    const onChoosePeer = jest.fn();
    render(
      <WorkingRouteBar
        route={['two%20words']}
        peerChoices={['peer%2Freader']}
        onRemoveHop={jest.fn()}
        onSelectHop={jest.fn()}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={onChoosePeer}
      />,
    );

    expect(screen.getByRole('button', { name: 'two words' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'peer/reader' }));
    expect(onChoosePeer).toHaveBeenCalledWith('peer%2Freader');
  });

  it('disables moving back at Self', () => {
    render(
      <WorkingRouteBar
        route={[]}
        peerChoices={null}
        onRemoveHop={jest.fn()}
        onSelectHop={jest.fn()}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={jest.fn()}
      />,
    );

    expect(screen.getByTitle('Move back one journal')).toBeDisabled();
  });
});
