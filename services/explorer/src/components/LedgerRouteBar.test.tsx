import React from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import LedgerRouteBar from './LedgerRouteBar';
import { LedgerHop } from '../types';

describe('LedgerRouteBar', () => {
  const hops: LedgerHop[] = [
    { key: 'local', kind: 'local', name: 'Self', snapshot: '42', maximum: 42 },
    { key: 'alice-1', kind: 'bridge', name: 'alice', snapshot: '150', maximum: 160 },
  ];

  it('shows the root sync button inline with the first hop', () => {
    render(
      <LedgerRouteBar
        hops={hops}
        peerChoices={null}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={jest.fn()}
        onStepSnapshot={jest.fn()}
        onRemoveHop={jest.fn()}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={jest.fn()}
      />,
    );

    expect(screen.getByTitle('Synchronize latest committed root')).toBeInTheDocument();
    expect(screen.getByDisplayValue('42')).toBeInTheDocument();
  });

  it('spins the same sync button without inserting a loading row', () => {
    render(
      <LedgerRouteBar
        hops={hops}
        rootIndex={42}
        onSynchronize={jest.fn()}
        isSynchronizing
        onSnapshotChange={jest.fn()}
        onStepSnapshot={jest.fn()}
        readOnlyRoute
      />,
    );

    const button = screen.getByTitle('Synchronize latest committed root');
    expect(button).toBeDisabled();
    expect(button).toHaveClass('synchronizing');
  });

  it('commits a multi-digit snapshot once on Enter without prefix navigation', () => {
    const onSnapshotChange = jest.fn();
    render(
      <LedgerRouteBar
        hops={hops}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={onSnapshotChange}
        onStepSnapshot={jest.fn()}
      />,
    );

    const input = screen.getByLabelText('Self snapshot');
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: '1' } });
    fireEvent.change(input, { target: { value: '12' } });
    expect(onSnapshotChange).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onSnapshotChange).toHaveBeenCalledTimes(1);
    expect(onSnapshotChange).toHaveBeenCalledWith(0, '12');
  });

  it('commits negative input on blur and restores the committed value on Escape', () => {
    const onSnapshotChange = jest.fn();
    const { rerender } = render(
      <LedgerRouteBar
        hops={hops}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={onSnapshotChange}
        onStepSnapshot={jest.fn()}
      />,
    );

    const input = screen.getByLabelText('alice snapshot');
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: '-12' } });
    fireEvent.blur(input);
    expect(onSnapshotChange).toHaveBeenLastCalledWith(1, '149');

    rerender(
      <LedgerRouteBar
        hops={[hops[0], { ...hops[1], snapshot: '149' }]}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={onSnapshotChange}
        onStepSnapshot={jest.fn()}
      />,
    );
    const committedInput = screen.getByLabelText('alice snapshot');
    fireEvent.focus(committedInput);
    fireEvent.change(committedInput, { target: { value: '-3' } });
    fireEvent.keyDown(committedInput, { key: 'Escape' });
    expect(committedInput).toHaveValue('149');
    expect(onSnapshotChange).toHaveBeenCalledTimes(1);
  });

  it('normalizes absolute, relative, and clamped snapshots on Enter and blur', () => {
    const onSnapshotChange = jest.fn();
    render(
      <LedgerRouteBar
        hops={hops}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={onSnapshotChange}
        onStepSnapshot={jest.fn()}
      />,
    );
    const input = screen.getByLabelText('alice snapshot');
    for (const [raw, expected, event] of [
      ['999', '160', 'Enter'],
      ['-1', '160', 'blur'],
      ['-2', '159', 'Enter'],
      ['-10', '151', 'blur'],
      ['-999', '0', 'Enter'],
    ]) {
      fireEvent.change(input, { target: { value: raw } });
      if (event === 'blur') fireEvent.blur(input);
      else fireEvent.keyDown(input, { key: 'Enter' });
      expect(onSnapshotChange).toHaveBeenLastCalledWith(1, expected);
    }
  });

  it('rejects latest, malformed, and unsafe public snapshot input without committing', () => {
    const onSnapshotChange = jest.fn();
    render(
      <LedgerRouteBar
        hops={hops}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={onSnapshotChange}
        onStepSnapshot={jest.fn()}
      />,
    );
    const input = screen.getByLabelText('alice snapshot');

    for (const [raw, event] of [
      ['latest', 'Enter'],
      ['not-an-index', 'blur'],
      ['9007199254740992', 'Enter'],
      ['-9007199254740992', 'blur'],
    ]) {
      fireEvent.focus(input);
      fireEvent.change(input, { target: { value: raw } });
      if (event === 'blur') fireEvent.blur(input);
      else fireEvent.keyDown(input, { key: 'Enter' });
      expect(input).toHaveValue('150');
    }
    expect(onSnapshotChange).not.toHaveBeenCalled();
  });

  it('does not clobber an active draft when the external route changes', () => {
    const onSnapshotChange = jest.fn();
    const { rerender } = render(
      <LedgerRouteBar
        hops={hops}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={onSnapshotChange}
        onStepSnapshot={jest.fn()}
      />,
    );

    const input = screen.getByLabelText('alice snapshot');
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: '-12' } });
    rerender(
      <LedgerRouteBar
        hops={[hops[0], { ...hops[1], snapshot: '-4' }]}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={onSnapshotChange}
        onStepSnapshot={jest.fn()}
      />,
    );
    expect(screen.getByLabelText('alice snapshot')).toHaveValue('-12');
  });

  it('keeps increment and decrement as immediate explicit actions', () => {
    const onStepSnapshot = jest.fn();
    render(
      <LedgerRouteBar
        hops={hops}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={jest.fn()}
        onStepSnapshot={onStepSnapshot}
      />,
    );

    const peerCard = screen.getByText('alice').closest('.hop-card-unified');
    const buttons = peerCard?.querySelectorAll('.stepper button');
    if (!buttons) throw new Error('Peer snapshot controls not found');
    fireEvent.click(buttons[0]);
    fireEvent.click(buttons[1]);
    expect(onStepSnapshot.mock.calls).toEqual([[1, 'older'], [1, 'newer']]);
  });

  it('selects any displayed route hop without changing its snapshot controls', () => {
    const onSelectHop = jest.fn();
    render(
      <LedgerRouteBar
        hops={hops}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={jest.fn()}
        onStepSnapshot={jest.fn()}
        onSelectHop={onSelectHop}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'alice' }));
    fireEvent.click(screen.getByRole('button', { name: 'Self' }));
    expect(onSelectHop.mock.calls).toEqual([[1], [0]]);
    expect(screen.getByLabelText('alice snapshot')).toHaveValue('150');
  });

  it('rejects empty peer snapshot input on blur', () => {
    const onSnapshotChange = jest.fn();

    render(
      <LedgerRouteBar
        hops={hops}
        peerChoices={null}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={onSnapshotChange}
        onStepSnapshot={jest.fn()}
        onRemoveHop={jest.fn()}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={jest.fn()}
      />,
    );

    const inputs = screen.getAllByRole('textbox');
    fireEvent.focus(inputs[1]);
    fireEvent.change(inputs[1], { target: { value: '' } });
    fireEvent.blur(inputs[1]);

    expect(inputs[1]).toHaveValue('150');
    expect(onSnapshotChange).not.toHaveBeenCalled();
  });

  it('shows bridge choices inside the chooser shell when open', () => {
    render(
      <LedgerRouteBar
        hops={hops}
        peerChoices={['bob', 'carol']}
        rootIndex={42}
        onSynchronize={jest.fn()}
        onSnapshotChange={jest.fn()}
        onStepSnapshot={jest.fn()}
        onRemoveHop={jest.fn()}
        onOpenPeerPicker={jest.fn()}
        onClosePeerPicker={jest.fn()}
        onChoosePeer={jest.fn()}
      />,
    );

    expect(screen.getByText('bob')).toBeInTheDocument();
    expect(screen.getByText('carol')).toBeInTheDocument();
    expect(screen.getByTitle('Exit bridge selection')).toBeInTheDocument();
  });
});
