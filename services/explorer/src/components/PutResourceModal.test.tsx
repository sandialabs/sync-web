import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import PutResourceModal from './PutResourceModal';

const renderModal = (onSubmit = jest.fn().mockResolvedValue(undefined)) => {
  render(<PutResourceModal open onClose={jest.fn()} onSubmit={onSubmit} />);
  return onSubmit;
};

const submit = async (name: string, body: string) => {
  fireEvent.change(screen.getByLabelText('Name'), { target: { value: name } });
  fireEvent.change(screen.getByRole('textbox', { name: /Text|Complete Scheme body/ }), {
    target: { value: body },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Put' }));
};

describe('PutResourceModal', () => {
  it('defaults to String and removes the JSON/Scheme format control', async () => {
    const onSubmit = renderModal();
    expect(screen.getByLabelText('String')).toBeChecked();
    expect(screen.queryByText('Input format')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('JSON')).not.toBeInTheDocument();

    await submit('note', 'plain text');
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('note', {
      mode: 'string', textValue: 'plain text',
    }));
  });

  it.each([
    ['Bytes', 'bytes', '#u(00 ff)'],
    ['Expression', 'expression', '(list 1 2)'],
    ['Object', 'object', '(define-class (counter))'],
  ])('submits %s as one exact Scheme body', async (label, mode, body) => {
    const onSubmit = renderModal();
    fireEvent.click(screen.getByLabelText(label));
    await submit(String(mode), body);
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(mode, {
      mode, schemeValue: body,
    }));
  });
});
