import React, { useEffect, useMemo, useRef, useState } from 'react';
import { DirectoryResult, ExplorerMode, ExplorerSelection, JournalPath, JournalResponse } from '../types';
import { JournalService } from '../services/JournalService';
import { compareSegmentedNames } from '../utils/sortKeys';

interface ExplorerContentProps {
  mode: ExplorerMode;
  selection: ExplorerSelection | null;
  journalService: JournalService | null;
  refreshKey: number;
  ledgerView: 'content' | 'proof';
  onLedgerViewToggle: () => void;
  onStageCreateFile: (path: JournalPath) => Promise<void>;
  onStageCreateDirectory: (path: JournalPath) => Promise<void>;
  onStageUploadFile: (path: JournalPath, file: File) => Promise<void>;
  onStageRename: (path: JournalPath, label: string) => Promise<void>;
  onStageDelete: (path: JournalPath, label: string) => Promise<void>;
  onSelectPath: (selection: ExplorerSelection) => void;
}

const ExplorerContent: React.FC<ExplorerContentProps> = ({
  mode,
  selection,
  journalService,
  refreshKey,
  ledgerView,
  onLedgerViewToggle,
  onStageCreateFile,
  onStageCreateDirectory,
  onStageUploadFile,
  onStageRename,
  onStageDelete,
  onSelectPath,
}) => {
  const [response, setResponse] = useState<JournalResponse | null>(null);
  const [responseKey, setResponseKey] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [editValue, setEditValue] = useState('');
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const uploadInputRef = useRef<HTMLInputElement | null>(null);
  const isEditingRef = useRef(false);
  const responseKeyRef = useRef<string | null>(null);

  useEffect(() => {
    isEditingRef.current = isEditing;
  }, [isEditing]);

  useEffect(() => {
    responseKeyRef.current = responseKey;
  }, [responseKey]);

  const selectionKey = useMemo(
    () => (selection ? JSON.stringify([selection.type, selection.path]) : null),
    [selection],
  );
  const currentResponse = responseKey === selectionKey ? response : null;

  const buildChildPath = (parentPath: JournalPath, childName: string): JournalPath => {
    if (!parentPath.includes('*state*')) {
      return parentPath;
    }

    return [...parentPath, childName];
  };

  useEffect(() => {
    if (!journalService || !selection) {
      setResponse(null);
      setIsEditing(false);
      setEditValue('');
      setActionNotice(null);
      return;
    }

    let active = true;
    const load = async () => {
      setIsLoading(true);
      try {
        const nextResponse = await journalService.get(selection.path);
        if (!active) {
          return;
        }
        if (isEditingRef.current && responseKeyRef.current === selectionKey) {
          return;
        }
        setResponse((prev) =>
          JSON.stringify(prev) === JSON.stringify(nextResponse) ? prev : nextResponse,
        );
        setResponseKey(selectionKey);
        setEditValue(JournalService.documentContentToText(nextResponse.content));
        setIsEditing(false);
        setActionNotice(null);
      } catch (error) {
        if (active) {
          const errorResponse = {
            content: `Error loading content: ${error instanceof Error ? error.message : 'Unknown error'}`,
            'pinned?': null,
            proof: { error: String(error) },
          };
          setResponse((prev) =>
            JSON.stringify(prev) === JSON.stringify(errorResponse) ? prev : errorResponse,
          );
          setResponseKey(selectionKey);
          setIsEditing(false);
        }
      } finally {
        if (active) {
          setIsLoading(false);
        }
      }
    };

    load();
    return () => {
      active = false;
    };
  }, [journalService, selection, selectionKey, mode === 'stage' ? refreshKey : 0]);

  const directory = useMemo<DirectoryResult | null>(() => {
    if (!currentResponse) {
      return null;
    }
    return JournalService.parseDirectoryResponse(currentResponse.content);
  }, [currentResponse]);

  const isPinnedValue = (value: JournalResponse['pinned?'] | null | undefined): boolean => {
    if (value == null) {
      return false;
    }
    if (typeof value === 'boolean') {
      return value;
    }
    if (Array.isArray(value)) {
      return value.length > 0;
    }
    return true;
  };

  const isPinned = useMemo(() => isPinnedValue(currentResponse?.['pinned?']), [currentResponse]);

  const directoryEntries = useMemo(() => {
    if (!currentResponse) {
      return null;
    }

    return JournalService.parseDirectoryEntries(currentResponse.content);
  }, [currentResponse]);

  const handleSave = async () => {
    if (!journalService || !selection) {
      return;
    }

    setIsLoading(true);
    try {
      await journalService.setText(selection.path, editValue);
      setIsEditing(false);
      const nextResponse = await journalService.get(selection.path);
      setResponse(nextResponse);
      setActionNotice('Saved');
    } catch (error) {
      alert(`Failed to save: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setIsLoading(false);
    }
  };

  const handleDownload = async () => {
    if (!journalService || !selection) {
      return;
    }

    try {
      const { blob, filename } = await journalService.download(selection.path);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = filename;
      anchor.click();
      URL.revokeObjectURL(url);
      setActionNotice(`Downloaded ${filename}`);
    } catch (error) {
      alert(`Failed to download: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  };

  const handlePinToggle = async () => {
    if (!journalService || !selection) {
      return;
    }
    setIsLoading(true);
    try {
      if (isPinned) {
        await journalService.unpin(selection.path);
      } else {
        await journalService.pin(selection.path);
      }

      const expectedPinned = !isPinned;
      setResponse((prev) =>
        prev
          ? {
              ...prev,
              'pinned?': expectedPinned,
            }
          : prev,
      );

      try {
        const nextResponse = await journalService.get(selection.path);
        setResponse(nextResponse);
      } catch {
        // Keep the optimistic pinned state if the immediate refresh fails.
      }
    } catch (error) {
      alert(`Failed to update pin: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setIsLoading(false);
    }
  };

  const openUploadDialog = () => uploadInputRef.current?.click();

  if (!selection) {
    return (
      <div className="empty-state">
        {mode === 'stage'
          ? 'Select a local document or directory to view its contents.'
          : 'Select a ledger document or directory to browse this route.'}
      </div>
    );
  }

  const extractedContent = JournalService.documentContentToText(currentResponse?.content);
  const title = String(selection.path[selection.path.length - 1] ?? 'item');

  return (
    <div className="content-viewer">
      <div className="content-header">
        <div className="content-path-container">
          <div className="content-path">
            {title}
            {mode === 'stage' && (
              <button
                className="button-inline-icon"
                title="Rename"
                onClick={() => void onStageRename(selection.path, title)}
              >✎</button>
            )}
          </div>
          {actionNotice && <div className="content-meta-note">{actionNotice}</div>}
        </div>
        <div className="content-actions">
          {mode === 'stage' && selection.type === 'directory' && (
            <>
              <button className="button button-secondary" onClick={() => void onStageCreateFile(selection.path)}>+ Document</button>
              <button className="button button-secondary" onClick={() => void onStageCreateDirectory(selection.path)}>+ Directory</button>
              <button className="button button-secondary" onClick={openUploadDialog}>Upload File</button>
              <input
                ref={uploadInputRef}
                type="file"
                style={{ display: 'none' }}
                onChange={(event) => {
                  const file = event.target.files?.[0];
                  if (file) {
                    void onStageUploadFile(selection.path, file);
                    event.target.value = '';
                  }
                }}
              />
              <button className="button button-secondary" onClick={() => void onStageDelete(selection.path, title)}>Delete</button>
            </>
          )}
          {mode === 'stage' && selection.type === 'file' && (
            <>
              <button className="button button-secondary" onClick={() => {
                if (isEditing) {
                  void handleSave();
                } else {
                  setIsEditing(true);
                }
              }}>
                {isEditing ? 'Save' : 'Edit'}
              </button>
              <button className="button button-secondary" onClick={handleDownload}>Download</button>
              <button className="button button-secondary" onClick={() => void onStageDelete(selection.path, title)}>Delete</button>
            </>
          )}
          {mode === 'ledger' && selection.type === 'file' && (
            <>
              <button
                className={isPinned ? 'button button-secondary' : 'button button-primary'}
                onClick={handlePinToggle}
              >
                {isPinned ? 'Unpin' : 'Pin'}
              </button>
              <button className="button button-secondary" onClick={handleDownload}>Download</button>
              <button className="button button-secondary" onClick={onLedgerViewToggle}>
                {ledgerView === 'content' ? 'Proof' : 'Content'}
              </button>
            </>
          )}
        </div>
      </div>

      <div className="content-body">
        {isLoading && !currentResponse ? (
          <div className="loading-spinner" />
        ) : mode === 'ledger' && selection.type === 'file' && ledgerView === 'proof' ? (
          <pre className="content-text">{JSON.stringify(currentResponse?.proof, null, 2)}</pre>
        ) : directory ? (
          <div className="directory-list">
            {(directoryEntries ?? [])
              .filter((item) => !JournalService.isReservedStateSegment(item.name))
              .sort((a, b) => {
                const leftRank = a.type === 'directory' ? 0 : 1;
                const rightRank = b.type === 'directory' ? 0 : 1;
                if (leftRank !== rightRank) {
                  return leftRank - rightRank;
                }
                return compareSegmentedNames(a.name, b.name);
              })
              .map((item) => (
                <button
                  key={item.name}
                  className="directory-item directory-item-button"
                  onClick={() => onSelectPath({
                    path: buildChildPath(selection.path, item.name),
                    type: item.type === 'directory' ? 'directory' : 'file',
                  })}
                >
                  <span className="directory-item-kind" aria-hidden="true">
                    {item.type === 'directory' ? '▣' : '▤'}
                  </span>
                  <span>{item.name}</span>
                </button>
              ))}
          </div>
        ) : mode === 'stage' && selection.type === 'file' && isEditing ? (
          <textarea
            className="content-editor"
            value={editValue}
            onChange={(event) => setEditValue(event.target.value)}
          />
        ) : (
          <pre className="content-text">
            {typeof extractedContent === 'string'
              ? extractedContent
              : JSON.stringify(extractedContent, null, 2)}
          </pre>
        )}
      </div>
    </div>
  );
};

export default ExplorerContent;
