import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { DirectoryResult, ExplorerMode, ExplorerSelection, JournalPath, JournalResponse } from '../types';
import { JournalService, ObjectInvocationResult } from '../services/JournalService';
import { compareSegmentedNames } from '../utils/sortKeys';
import { pathSegmentIdentity } from '../utils/pathUtils';
import { encodeLedgerRawSelection, encodeStageRawSelection } from '../utils/rawUrl';
import ObjectPane from './ObjectPane';

const ACCESS_MESSAGE = 'Unable to load this location. Check that the selected journal has granted access to this user and path.';
const SNAPSHOT_MESSAGE = 'The selected ledger snapshot is unavailable. It may still be committing or may no longer be retained.';
const displayPathSegment = (segment: unknown): string => {
  if (segment === '*state*') return 'State';
  if (segment === '*bridge*') return 'Bridges';
  return JournalService.decodePathSegment(String(segment));
};

const isPinnedValue = (value: JournalResponse['pinned?'] | null | undefined): boolean => {
  if (value == null) return false;
  if (typeof value === 'boolean') return value;
  if (Array.isArray(value)) return value.length > 0;
  return true;
};

interface ExplorerContentProps {
  mode: ExplorerMode;
  selection: ExplorerSelection | null;
  journalService: JournalService | null;
  refreshKey: number;
  ledgerView: 'content' | 'proof';
  stageReadOnly?: boolean;
  readPath?: JournalPath | null;
  pinJournalService?: JournalService | null;
  pinPath?: JournalPath | null;
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
  readPath = null,
  pinJournalService = null,
  pinPath = null,
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
  const [verifiedPinned, setVerifiedPinned] = useState<boolean | null>(null);
  const [objectMetadata, setObjectMetadata] = useState<string | null>(null);
  const [objectApi, setObjectApi] = useState<ObjectInvocationResult | null>(null);
  const [objectStatus, setObjectStatus] = useState<'checking' | 'object' | 'value'>('checking');
  const uploadInputRef = useRef<HTMLInputElement | null>(null);
  const isEditingRef = useRef(false);
  const responseKeyRef = useRef<string | null>(null);
  const selectionGenerationRef = useRef(0);

  const setEditing = useCallback((value: boolean) => {
    isEditingRef.current = value;
    setIsEditing(value);
  }, []);

  useEffect(() => {
    responseKeyRef.current = responseKey;
  }, [responseKey]);

  const operationPath = readPath ?? selection?.path ?? null;
  const selectionKey = useMemo(
    () => (selection ? JSON.stringify([selection.type, selection.path, operationPath]) : null),
    [operationPath, selection],
  );
  const currentResponse = responseKey === selectionKey ? response : null;
  const structuralLedgerSelection = mode === 'ledger'
    && operationPath !== null && !operationPath.includes('*state*');
  const refreshDependency = mode === 'stage' ? refreshKey : 0;

  useEffect(() => {
    const generation = ++selectionGenerationRef.current;
    if (!journalService || !selection || !operationPath || structuralLedgerSelection) {
      setResponse(null);
      setEditing(false);
      setEditValue('');
      setActionNotice(null);
      setActionError(null);
      setLoadError(null);
      setVerifiedPinned(null);
      setObjectMetadata(null);
      setObjectApi(null);
      setObjectStatus(selection?.type === 'directory' ? 'value' : 'checking');
      return;
    }

    let active = true;
    const current = () => active && selectionGenerationRef.current === generation;
    const objectProbe = selection.type !== 'directory'
      ? journalService.probeObjectApi({
          path: operationPath,
          historical: mode === 'ledger',
        })
      : null;
    const objectResult = objectProbe
      ? objectProbe.then(() => journalService.invokeObject({
          path: operationPath,
          method: '*api*',
          argumentsExpression: '()',
          readOnly: true,
          historical: mode === 'ledger',
        })).then(
          (result) => result,
          () => null,
        )
      : null;
    const pinResult = mode === 'ledger' && selection.type !== 'directory'
        && pinJournalService && pinPath
      ? pinJournalService.get(pinPath, { pinned: true, proof: false })
      : null;

    setIsLoading(true);
    setLoadError(null);
    setVerifiedPinned(null);
    setObjectMetadata(null);
    setObjectApi(null);
    setObjectStatus(selection.type === 'directory' ? 'value' : 'checking');

    if (pinResult) {
      void pinResult.then((pinEvidence) => {
        if (current()) setVerifiedPinned(isPinnedValue(pinEvidence['pinned?']));
      }).catch(() => {
        if (current()) setVerifiedPinned(null);
      });
    }

    const load = async () => {
      try {
        const nextResponse = await journalService.get(operationPath, {
          pinned: mode === 'ledger',
          proof: mode === 'ledger' && ledgerView === 'proof',
          ...(mode === 'ledger' && selection.type !== 'directory' ? { selectedIndexes: true } : {}),
        });
        if (!current()) return;
        if (isEditingRef.current && responseKeyRef.current === selectionKey) return;

        setResponse((prev) =>
          JSON.stringify(prev) === JSON.stringify(nextResponse) ? prev : nextResponse,
        );
        setResponseKey(selectionKey);
        setEditValue(JournalService.documentContentToText(nextResponse.content));
        setEditing(false);
        setActionNotice(null);
        setActionError(null);
        setLoadError(null);
        setIsLoading(false);

        if (objectResult) {
          void objectResult.then((apiResult) => {
            if (!current()) return;
            if (!apiResult) {
              setObjectMetadata(null);
              setObjectApi(null);
              setObjectStatus('value');
              return;
            }
            setObjectMetadata(JournalService.documentContentToText(nextResponse.content));
            setObjectApi(apiResult);
            setObjectStatus('object');
          });
        }
      } catch (error) {
        if (current() && objectProbe && objectResult) {
          try {
            await objectProbe;
            const metadataResultPromise = journalService.invokeObject({
              path: operationPath,
              method: '',
              argumentsExpression: '()',
              readOnly: true,
              historical: mode === 'ledger',
            });
            const [apiResult, metadataResult] = await Promise.all([
              objectResult, metadataResultPromise,
            ]);
            if (!apiResult) throw new Error('Object API probe failed');
            if (!current()) return;
            const metadata = JournalService.documentContentToText(metadataResult.result);
            setResponse({ content: metadataResult.result });
            setObjectMetadata(metadata);
            setObjectApi(apiResult);
            setObjectStatus('object');
            setResponseKey(selectionKey);
            setEditValue(metadata);
            setEditing(false);
            setActionNotice(null);
            setActionError(null);
            setLoadError(null);
            return;
          } catch {
            // The normal read error remains authoritative when object dispatch also fails.
          }
        }
        if (current()) {
          if (isEditingRef.current && responseKeyRef.current === selectionKey) return;
          setResponse(null);
          setResponseKey(selectionKey);
          setEditing(false);
          setActionNotice(null);
          setActionError(null);
          setObjectMetadata(null);
          setObjectApi(null);
          setObjectStatus('value');
          setLoadError(JournalService.isSnapshotUnavailable(error) ? SNAPSHOT_MESSAGE : ACCESS_MESSAGE);
        }
      } finally {
        if (current()) setIsLoading(false);
      }
    };

    void load();
    return () => {
      active = false;
      if (selectionGenerationRef.current === generation) {
        selectionGenerationRef.current += 1;
      }
    };
  }, [
    journalService, selection, selectionKey, refreshDependency, mode, ledgerView,
    structuralLedgerSelection, setEditing, pinJournalService, pinPath, operationPath,
  ]);

  const directory = useMemo<DirectoryResult | null>(() => {
    if (!currentResponse) {
      return null;
    }
    return JournalService.parseDirectoryResponse(currentResponse.content);
  }, [currentResponse]);

  const isPinned = mode === 'ledger' ? verifiedPinned === true : isPinnedValue(currentResponse?.['pinned?']);
  const isObject = selection?.type === 'object' || objectStatus === 'object';

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
      label: displayPathSegment(segment),
      path: selection.path.slice(0, stateIndex + index + 1),
    }));
  }, [selection]);

  const handleSave = async () => {
    if (!journalService || !selection || !operationPath) {
      return;
    }

    setIsLoading(true);
    setActionError(null);
    try {
      await journalService.setText(operationPath, editValue);
      const nextResponse = await journalService.get(operationPath, {
        pinned: mode === 'ledger',
        proof: mode === 'ledger' && ledgerView === 'proof',
      });
      setResponse(nextResponse);
      setObjectMetadata(null);
      setObjectApi(null);
      setEditValue(JournalService.documentContentToText(nextResponse.content));
      setEditing(false);
      setActionNotice('Saved');
    } catch (error) {
      setActionError(`Save failed: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setIsLoading(false);
    }
  };

  const handleDownload = async () => {
    if (!journalService || !selection || !operationPath) {
      return;
    }

    try {
      const { blob, filename } = await journalService.download(operationPath);
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
    if (!journalService || !selection || !operationPath || !pinJournalService || !pinPath) return;
    const generation = selectionGenerationRef.current;
    const current = () => selectionGenerationRef.current === generation;
    setIsLoading(true);
    setActionError(null);
    setActionNotice(null);
    const expectedPinned = !isPinned;
    try {
      if (expectedPinned) await pinJournalService.pin(pinPath);
      else await pinJournalService.unpin(pinPath);

      const localEvidence = await pinJournalService.get(pinPath, { pinned: true, proof: false });
      if (!current()) return;
      if (isPinnedValue(localEvidence['pinned?']) !== expectedPinned) {
        throw new Error(`post-${expectedPinned ? 'pin' : 'unpin'} readback did not confirm the requested state`);
      }
      if (expectedPinned && pinPath.indexOf('*state*') > 1) {
        await pinJournalService.verifyPinnedInventories(pinPath);
        if (!current()) return;
      }
      const routedEvidence = await journalService.get(operationPath, {
        pinned: true,
        proof: ledgerView === 'proof',
        selectedIndexes: true,
      });
      if (!current()) return;
      if (JSON.stringify(localEvidence.content) !== JSON.stringify(routedEvidence.content)) {
        throw new Error('local and terminal-provider evidence disagree');
      }
      setResponse(routedEvidence);
      setVerifiedPinned(expectedPinned);
      setActionNotice(expectedPinned ? 'Pinned with local and terminal-provider readback' : 'Unpinned with exact readback');
    } catch (error) {
      if (current()) {
        setVerifiedPinned(null);
        setActionError(`Pin state is indeterminate: ${error instanceof Error ? error.message : 'Unknown error'}`);
      }
    } finally {
      if (current()) setIsLoading(false);
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

  if (structuralLedgerSelection) {
    return (
      <div className="content-viewer">
        <div className="content-header">
          <div className="content-path-container">
            <div className="content-path">
              {displayPathSegment(selection.path[selection.path.length - 1] ?? 'index')}
            </div>
          </div>
        </div>
        <div className="empty-state">No accessible contents</div>
      </div>
    );
  }

  const extractedContent = JournalService.documentContentToText(currentResponse?.content);
  const title = displayPathSegment(selection.path[selection.path.length - 1] ?? 'item');
  const rawSelection = currentResponse && selection.type !== 'directory'
    ? mode === 'stage'
      ? encodeStageRawSelection(journalService?.getFederationContext()?.route ?? [], operationPath ?? selection.path)
      : encodeLedgerRawSelection(operationPath ?? selection.path, currentResponse.indexes)
    : null;
  const rawHref = rawSelection && journalService ? journalService.rawUrl(rawSelection) : null;

  return (
    <div className="content-viewer">
      <div className="content-header">
        <div className="content-path-container">
          <div className="content-path">
            {title}
            {mode === 'stage' && !stageReadOnly && (
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
              <button className="button button-secondary" onClick={() => void onStageCreateFile(selection.path)}>+ Put</button>
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
          {!loadError && mode === 'stage' && selection.type !== 'directory' && (
            <>
              {rawHref && (
                <a className="button button-secondary" href={rawHref} target="_blank" rel="noopener noreferrer">Raw</a>
              )}
              {!stageReadOnly && !isObject && objectStatus === 'value' && (
                <button className="button button-secondary" onClick={() => {
                  if (isEditing) {
                    void handleSave();
                  } else {
                    setActionError(null);
                    setEditing(true);
                  }
                }}>
                  {isEditing ? 'Save' : 'Edit'}
                </button>
              )}
              <button className="button button-secondary" onClick={handleDownload}>Download</button>
              {!stageReadOnly && (
                <button className="button button-secondary" onClick={() => void onStageDelete(selection.path, title)}>Delete</button>
              )}
            </>
          )}
          {!loadError && mode === 'ledger' && selection.type !== 'directory' && (
            <>
              <button className="button button-secondary" onClick={onLedgerViewToggle}>
                {ledgerView === 'content' ? 'Proof' : 'Content'}
              </button>
              {rawHref && (
                <a className="button button-secondary" href={rawHref} target="_blank" rel="noopener noreferrer">Raw</a>
              )}
              <button
                className={isPinned ? 'button button-secondary' : 'button button-primary'}
                onClick={handlePinToggle}
                disabled={verifiedPinned === null}
                title={verifiedPinned === null ? 'Pin status requires exact local readback' : undefined}
              >
                {verifiedPinned === null ? 'Pin status unavailable' : isPinned ? 'Unpin' : 'Pin'}
              </button>
              <button className="button button-secondary" onClick={handleDownload}>Download</button>
            </>
          )}
        </div>
      </div>

      <div className="content-body">
        {isLoading && !currentResponse ? (
          <div className="loading-spinner" />
        ) : loadError ? (
          <div className="content-load-error" role="alert">{loadError}</div>
        ) : mode === 'ledger' && selection.type !== 'directory' && ledgerView === 'proof' ? (
          <pre className="content-text">{JSON.stringify(currentResponse?.proof, null, 2)}</pre>
        ) : directory ? (
          <div className="directory-list">
            {(directoryEntries ?? [])
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
                  key={pathSegmentIdentity(item.pathSegment ?? item.name)}
                  className="directory-item directory-item-button"
                  onClick={() => onSelectPath({
                    path: [
                      ...selection.path,
                      item.pathSegment ?? JournalService.encodePathSegment(item.name),
                    ],
                    type: item.type === 'directory' ? 'directory'
                      : item.type === 'object' ? 'object' : 'file',
                  })}
                >
                  <span className="directory-item-kind" aria-hidden="true">
                    {item.type === 'directory' ? '▣' : item.type === 'object' ? '◆' : '▤'}
                  </span>
                  <span>{item.name}</span>
                </button>
              ))}
          </div>
        ) : mode === 'stage' && selection.type === 'file' && isEditing ? (
          <textarea
            className="content-editor"
            aria-label="Document content"
            value={editValue}
            onChange={(event) => setEditValue(event.target.value)}
          />
        ) : isObject && journalService && objectMetadata && objectApi ? (
          <ObjectPane
            journalService={journalService}
            path={operationPath ?? selection.path}
            historical={mode === 'ledger'}
            metadata={objectMetadata}
            apiResult={objectApi}
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
