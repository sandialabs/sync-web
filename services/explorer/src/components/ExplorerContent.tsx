import React, { useEffect, useMemo, useRef, useState } from 'react';
import { DirectoryResult, ExplorerMode, ExplorerSelection, JournalPath, JournalResponse } from '../types';
import { JournalService } from '../services/JournalService';
import { compareSegmentedNames } from '../utils/sortKeys';
import RawDocumentView from './RawDocumentView';

const ACCESS_MESSAGE = 'Unable to load this location. Check that the selected journal has granted access to this user and path.';
const SNAPSHOT_MESSAGE = 'The selected ledger snapshot is unavailable. It may still be committing or may no longer be retained.';

interface ExplorerContentProps {
  mode: ExplorerMode;
  selection: ExplorerSelection | null;
  journalService: JournalService | null;
  refreshKey: number;
  ledgerView: 'content' | 'proof';
  stageReadOnly?: boolean;
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
  stageReadOnly = false,
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
  const [actionError, setActionError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [rawView, setRawView] = useState(false);
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
  const refreshDependency = mode === 'stage' ? refreshKey : 0;

  useEffect(() => {
    if (!journalService || !selection) {
      setResponse(null);
      setIsEditing(false);
      setEditValue('');
      setActionNotice(null);
      setActionError(null);
      setLoadError(null);
      setRawView(false);
      return;
    }

    let active = true;
    const load = async () => {
      setIsLoading(true);
      setLoadError(null);
      try {
        const nextResponse = await journalService.get(selection.path, {
          pinned: mode === 'ledger',
          proof: mode === 'ledger' && ledgerView === 'proof',
        });
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
        setActionError(null);
        setLoadError(null);
      } catch (error) {
        if (active) {
          setResponse(null);
          setResponseKey(selectionKey);
          setIsEditing(false);
          setActionNotice(null);
          setActionError(null);
          setLoadError(JournalService.isSnapshotUnavailable(error) ? SNAPSHOT_MESSAGE : ACCESS_MESSAGE);
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
  }, [journalService, selection, selectionKey, refreshDependency, mode, ledgerView]);

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

  const breadcrumbs = useMemo(() => {
    if (!selection) return [];
    const stateIndex = selection.path.lastIndexOf('*state*');
    if (stateIndex < 0) return [];
    return selection.path.slice(stateIndex).map((segment, index) => ({
      label: JournalService.decodePathSegment(String(segment)),
      path: selection.path.slice(0, stateIndex + index + 1),
    }));
  }, [selection]);

  const handleSave = async () => {
    if (!journalService || !selection) {
      return;
    }

    setIsLoading(true);
    setActionError(null);
    try {
      await journalService.setText(selection.path, editValue);
      const nextResponse = await journalService.get(selection.path, {
        pinned: mode === 'ledger',
        proof: mode === 'ledger' && ledgerView === 'proof',
      });
      setResponse(nextResponse);
      setEditValue(JournalService.documentContentToText(nextResponse.content));
      setIsEditing(false);
      setActionNotice('Saved');
    } catch (error) {
      setActionError(`Save failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
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
        const nextResponse = await journalService.get(selection.path, {
          pinned: mode === 'ledger',
          proof: mode === 'ledger' && ledgerView === 'proof',
        });
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
  const title = JournalService.decodePathSegment(String(selection.path[selection.path.length - 1] ?? 'item'));

  return (
    <div className="content-viewer">
      <div className="content-header">
        <div className="content-path-container">
          <div className="content-path">
            {title}
            {mode === 'stage' && !rawView && !stageReadOnly && (
              <button
                className="button-inline-icon"
                title="Rename"
                onClick={() => void onStageRename(selection.path, title)}
              >✎</button>
            )}
          </div>
          <nav className="content-breadcrumb" aria-label="Content path">
            {breadcrumbs.map((breadcrumb, index) => {
              const current = index === breadcrumbs.length - 1;
              return (
                <React.Fragment key={JSON.stringify(breadcrumb.path)}>
                  {index > 0 && <span className="content-breadcrumb-separator" aria-hidden="true">/</span>}
                  {current ? (
                    <span className="content-breadcrumb-current" aria-current="page">
                      {breadcrumb.label}
                    </span>
                  ) : (
                    <button
                      type="button"
                      className="content-breadcrumb-link"
                      onClick={() => onSelectPath({ path: breadcrumb.path, type: 'directory' })}
                    >
                      {breadcrumb.label}
                    </button>
                  )}
                </React.Fragment>
              );
            })}
          </nav>
          {actionNotice && <div className="content-meta-note">{actionNotice}</div>}
          {actionError && <div className="content-action-error" role="alert">{actionError}</div>}
        </div>
        <div className="content-actions">
          {!loadError && mode === 'stage' && !stageReadOnly && selection.type === 'directory' && (
            <>
              <button className="button button-secondary" onClick={() => void onStageCreateFile(selection.path)}>+ Document</button>
              <button className="button button-secondary" onClick={() => void onStageCreateDirectory(selection.path)}>+ Directory</button>
              <button className="button button-secondary" onClick={openUploadDialog}>Upload Document</button>
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
          {!loadError && mode === 'stage' && selection.type === 'file' && (
            <>
              <button className="button button-secondary" onClick={() => setRawView(false)}>Content</button>
              <button className="button button-secondary" onClick={() => setRawView(true)}>Raw</button>
              {!rawView && !stageReadOnly && (
                <button className="button button-secondary" onClick={() => {
                  if (isEditing) {
                    void handleSave();
                  } else {
                    setActionError(null);
                    setIsEditing(true);
                  }
                }}>
                  {isEditing ? 'Save' : 'Edit'}
                </button>
              )}
              <button className="button button-secondary" onClick={handleDownload}>Download</button>
              {!rawView && !stageReadOnly && (
                <button className="button button-secondary" onClick={() => void onStageDelete(selection.path, title)}>Delete</button>
              )}
            </>
          )}
          {!loadError && mode === 'ledger' && selection.type === 'file' && (
            <>
              <button className="button button-secondary" onClick={() => {
                setRawView(false);
                if (ledgerView === 'proof') onLedgerViewToggle();
              }}>Content</button>
              <button className="button button-secondary" onClick={() => setRawView(true)}>Raw</button>
              <button
                className={isPinned ? 'button button-secondary' : 'button button-primary'}
                onClick={handlePinToggle}
              >
                {isPinned ? 'Unpin' : 'Pin'}
              </button>
              <button className="button button-secondary" onClick={handleDownload}>Download</button>
              <button className="button button-secondary" onClick={() => {
                setRawView(false);
                if (ledgerView === 'content') onLedgerViewToggle();
              }}>Proof</button>
            </>
          )}
        </div>
      </div>

      <div className="content-body">
        {isLoading && !currentResponse ? (
          <div className="loading-spinner" />
        ) : loadError ? (
          <div className="content-load-error" role="alert">{loadError}</div>
        ) : selection.type === 'file' && rawView ? (
          <RawDocumentView content={currentResponse?.content} filename={title} />
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
                    path: [
                      ...selection.path,
                      item.pathSegment ?? JournalService.encodePathSegment(item.name),
                    ],
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
        ) : mode === 'stage' && selection.type === 'file' && isEditing && !rawView ? (
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
