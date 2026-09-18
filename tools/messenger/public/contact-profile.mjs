import { validateProfile } from "./profile.mjs";

export class ContactProfileError extends Error {}
export class ContactProfileTimeoutError extends ContactProfileError {
  constructor() { super("Profile fetch timed out"); this.name = "ProfileFetchTimeout"; }
}

const AUTHENTICATION_SCOPE = "route-owner-journal-path-current";
const TERMINAL_CONTINUITY = "unproven";

const SUBJECT_SYMBOL_RE = /^[A-Za-z0-9_.*+!<>=?/-]+$/;
const ROUTE_COMPONENT_RE = /^[A-Za-z0-9_.*+!<>=?-]+$/;
const CONTACT_ID_RE = /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/;
function subjectSymbol(value) { return typeof value === "string" && SUBJECT_SYMBOL_RE.test(value) && ![".", ".."].includes(value); }
function routeComponent(value) { return typeof value === "string" && ROUTE_COMPONENT_RE.test(value) && ![".", ".."].includes(value); }

function canonicalEndpoint(contact) {
  const qualified = `${contact.identity}@${contact.journal}`;
  if (typeof contact.contactId !== "string" || !CONTACT_ID_RE.test(contact.contactId)
    || !subjectSymbol(contact.identity) || !subjectSymbol(contact.journal) || !subjectSymbol(contact.owner)
    || !Array.isArray(contact.route) || contact.route.length === 0 || !contact.route.every(routeComponent)
    || contact.route.join("/").split("/").join("/") !== contact.route.join("/")
    || !Array.isArray(contact.incomingPrincipal) || contact.incomingPrincipal.length < 3
    || !contact.incomingPrincipal.every(subjectSymbol)
    || contact.incomingPrincipal.at(-2) !== "*state*" || contact.incomingPrincipal.at(-1) !== contact.identity) {
    throw new ContactProfileError("Contact profile relationship is not canonically bound");
  }
  return qualified;
}

function exactSubject(contact) {
  return {
    contactId: contact.contactId,
    qualified: canonicalEndpoint(contact),
    owner: contact.owner,
    journal: contact.journal,
    route: [...contact.route],
    path: ["profile.scm"],
  };
}

export function contactRelationshipFingerprint(contact) {
  return JSON.stringify(exactSubject(contact));
}

function sameSubject(left, right) {
  return left?.contactId === right.contactId && left?.qualified === right.qualified
    && left?.owner === right.owner && left?.journal === right.journal
    && JSON.stringify(left?.route) === JSON.stringify(right.route)
    && JSON.stringify(left?.path) === JSON.stringify(right.path);
}

function bytesFromHex(hex) {
  if (typeof hex !== "string" || hex.length > 8192 || hex.length % 2 || /[^0-9a-f]/i.test(hex)) {
    throw new ContactProfileError("Stored profile bytes are invalid");
  }
  return Uint8Array.from(hex.match(/../g) ?? [], (part) => Number.parseInt(part, 16));
}

async function sha256Hex(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function exactProfile(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export async function contactSnapshot(contact, current, previous) {
  const subject = exactSubject(contact);
  const rawHex = current?.value?.["*type/byte-vector*"];
  const raw = bytesFromHex(rawHex);
  const rawSha256 = await sha256Hex(raw);
  const profile = validateProfile(raw, { owner: subject.owner, journal: subject.journal });
  if (current.rawSha256 !== rawSha256
    || current.stale !== false || new Date(current.observedAt).toISOString() !== current.observedAt
    || !exactProfile(current.profile, profile)) throw new ContactProfileError("Current profile evidence is inconsistent");
  if (sameSubject(previous?.subject, subject)) {
    if (!Number.isSafeInteger(previous.profile?.revision) || !/^[0-9a-f]{64}$/.test(previous.source?.rawSha256)) {
      throw new ContactProfileError("Previous profile snapshot is invalid");
    }
    if (profile.revision < previous.profile.revision) throw new ContactProfileError("Profile revision rollback rejected");
    if (profile.revision === previous.profile.revision && rawSha256 !== previous.source.rawSha256) {
      throw new ContactProfileError("Profile revision fork rejected");
    }
  }
  const snapshot = {
    version: 1,
    classification: "untrusted-owner-authored-descriptive-metadata",
    subject,
    source: {
      path: ["profile.scm"], view: "current-get", authenticationScope: AUTHENTICATION_SCOPE,
      terminalJournalContinuity: TERMINAL_CONTINUITY, rawSha256, rawBytes: raw.length,
      observedAt: current.observedAt,
    },
    rawHex: rawHex.toLowerCase(),
    profile,
  };
  return snapshot;
}

export async function restoreContactSnapshot(contact, snapshot, status) {
  const subject = exactSubject(contact);
  if (snapshot?.version !== 1 || snapshot.classification !== "untrusted-owner-authored-descriptive-metadata"
    || !sameSubject(snapshot.subject, subject) || JSON.stringify(snapshot.source?.path) !== '["profile.scm"]'
    || snapshot.source?.view !== "current-get" || snapshot.source?.authenticationScope !== AUTHENTICATION_SCOPE
    || snapshot.source?.terminalJournalContinuity !== TERMINAL_CONTINUITY
    || !/^[0-9a-f]{64}$/.test(snapshot.source?.rawSha256)
    || !Number.isSafeInteger(snapshot.source?.rawBytes) || snapshot.source.rawBytes < 1 || snapshot.source.rawBytes > 4096
    || typeof snapshot.rawHex !== "string" || snapshot.rawHex.length > 8192
    || new Date(snapshot.source?.observedAt).toISOString() !== snapshot.source.observedAt) {
    throw new ContactProfileError("Stored profile snapshot is invalid or belongs to another relationship");
  }
  const raw = bytesFromHex(snapshot.rawHex);
  const rawSha256 = await sha256Hex(raw);
  const profile = validateProfile(raw, { owner: subject.owner, journal: subject.journal });
  if (raw.length !== snapshot.source.rawBytes || rawSha256 !== snapshot.source.rawSha256 || !exactProfile(profile, snapshot.profile)) {
    throw new ContactProfileError("Stored profile snapshot does not match its exact source bytes");
  }
  const exactStatus = statusMatchesContact(contact, status) && status.snapshotRawSha256 === rawSha256 ? status : undefined;
  return {
    profile, value: { "*type/byte-vector*": snapshot.rawHex },
    observedAt: snapshot.source.observedAt, stale: exactStatus?.stale === true, rawSha256,
    authenticationScope: AUTHENTICATION_SCOPE, terminalJournalContinuity: TERMINAL_CONTINUITY,
  };
}

export function successfulContactStatus(contact, snapshot, previous, attemptedAt = new Date().toISOString(), attemptId = crypto.randomUUID()) {
  const subject = exactSubject(contact);
  if (!sameSubject(snapshot?.subject, subject)) throw new ContactProfileError("Cannot record status for another relationship");
  const previousSuccessAt = sameSubject(previous?.subject, subject) ? previous.lastSuccessAt ?? null : null;
  return {
    version: 1, subject, authenticationScope: AUTHENTICATION_SCOPE, terminalJournalContinuity: TERMINAL_CONTINUITY,
    lastAttemptAt: attemptedAt, attemptId, lastAttempt: "accepted",
    lastSuccessAt: attemptedAt, stale: false, errorCode: null,
    snapshotRawSha256: snapshot.source.rawSha256, previousSuccessAt,
  };
}

export function failedContactStatus(contact, snapshot, previous, error, attemptedAt = new Date().toISOString(), attemptId = crypto.randomUUID()) {
  const subject = exactSubject(contact);
  const errorCode = error instanceof Error && error.name ? error.name.slice(0, 80) : "ProfileRefreshError";
  const lastSuccessAt = sameSubject(previous?.subject, subject) ? previous.lastSuccessAt ?? null : null;
  const boundSnapshot = sameSubject(snapshot?.subject, subject) ? snapshot : undefined;
  return {
    version: 1, subject, authenticationScope: AUTHENTICATION_SCOPE, terminalJournalContinuity: TERMINAL_CONTINUITY,
    lastAttemptAt: attemptedAt, attemptId, lastAttempt: "unavailable",
    lastSuccessAt, stale: Boolean(boundSnapshot), errorCode,
    snapshotRawSha256: boundSnapshot?.source?.rawSha256 ?? null,
  };
}

export function statusMatchesContact(contact, status) {
  try {
    const timestamp = (value) => value === null || (typeof value === "string" && new Date(value).toISOString() === value);
    return status?.version === 1 && sameSubject(status.subject, exactSubject(contact))
      && status.authenticationScope === AUTHENTICATION_SCOPE && status.terminalJournalContinuity === TERMINAL_CONTINUITY
      && typeof status.attemptId === "string" && status.attemptId.length > 0 && status.attemptId.length <= 160
      && new Date(status.lastAttemptAt).toISOString() === status.lastAttemptAt
      && ["accepted", "unavailable", "rejected"].includes(status.lastAttempt)
      && timestamp(status.lastSuccessAt) && typeof status.stale === "boolean"
      && (status.errorCode === null || (typeof status.errorCode === "string" && status.errorCode.length > 0 && status.errorCode.length <= 80))
      && (status.snapshotRawSha256 === null || /^[0-9a-f]{64}$/.test(status.snapshotRawSha256));
  } catch { return false; }
}

export function validatedContactStatus(contact, status, snapshot) {
  if (!statusMatchesContact(contact, status)) return null;
  const hasSnapshot = snapshot?.version === 1 && sameSubject(snapshot.subject, exactSubject(contact))
    && /^[0-9a-f]{64}$/.test(snapshot.source?.rawSha256);
  if (!hasSnapshot) {
    return status.lastAttempt === "unavailable" && status.stale === false && status.snapshotRawSha256 === null
      && status.lastSuccessAt === null && status.errorCode !== null ? status : null;
  }
  if (status.snapshotRawSha256 !== snapshot.source.rawSha256) return null;
  if (status.lastAttempt === "accepted") {
    return status.stale === false && status.errorCode === null && status.lastSuccessAt !== null ? status : null;
  }
  return status.stale === true && status.errorCode !== null && status.lastSuccessAt !== null ? status : null;
}

function compareAttempts(left, right) {
  const time = Date.parse(left?.lastAttemptAt || 0) - Date.parse(right?.lastAttemptAt || 0);
  if (time) return Math.sign(time);
  return String(left?.attemptId || "").localeCompare(String(right?.attemptId || ""));
}

function statusForSelectedSnapshot(contact, status, snapshot, overrides = {}) {
  const subject = exactSubject(contact);
  return {
    ...status,
    version: 1,
    subject,
    authenticationScope: AUTHENTICATION_SCOPE,
    terminalJournalContinuity: TERMINAL_CONTINUITY,
    stale: overrides.stale ?? status?.stale === true,
    errorCode: overrides.errorCode ?? status?.errorCode ?? null,
    lastAttempt: overrides.lastAttempt ?? status?.lastAttempt ?? "unavailable",
    lastSuccessAt: overrides.lastSuccessAt ?? status?.lastSuccessAt ?? null,
    snapshotRawSha256: snapshot?.source?.rawSha256 ?? null,
  };
}

async function validatedCandidate(contact, value) {
  if (!value?.snapshot) return undefined;
  await restoreContactSnapshot(contact, value.snapshot, value.status);
  return value.snapshot;
}

export async function mergeContactProfileState(contact, incumbent, candidate) {
  let incumbentSnapshot;
  let candidateSnapshot;
  try { incumbentSnapshot = await validatedCandidate(contact, incumbent); } catch { /* Invalid local cache is not incumbent state. */ }
  try { candidateSnapshot = await validatedCandidate(contact, candidate); } catch { /* Invalid candidate cannot replace current. */ }
  let incumbentStatus = statusMatchesContact(contact, incumbent?.status)
    && (!incumbentSnapshot || incumbent.status.snapshotRawSha256 === incumbentSnapshot.source.rawSha256)
    ? incumbent.status : undefined;
  const candidateStatus = statusMatchesContact(contact, candidate?.status)
    && (!candidateSnapshot || candidate.status.snapshotRawSha256 === candidateSnapshot.source.rawSha256)
    ? candidate.status : undefined;
  if (!incumbentStatus && incumbentSnapshot) {
    incumbentStatus = {
      version: 1, subject: exactSubject(contact), authenticationScope: AUTHENTICATION_SCOPE,
      terminalJournalContinuity: TERMINAL_CONTINUITY, lastAttemptAt: incumbentSnapshot.source.observedAt,
      attemptId: `snapshot:${incumbentSnapshot.source.rawSha256}`, lastAttempt: "accepted",
      lastSuccessAt: incumbentSnapshot.source.observedAt, stale: false, errorCode: null,
      snapshotRawSha256: incumbentSnapshot.source.rawSha256,
    };
  }
  if (!candidateStatus) candidateSnapshot = undefined;
  const candidateIsLatest = compareAttempts(candidateStatus, incumbentStatus) > 0;

  if (!candidateSnapshot) {
    const status = candidateIsLatest && candidateStatus ? candidateStatus : incumbentStatus;
    return {
      ...(incumbentSnapshot ? { snapshot: incumbentSnapshot } : {}),
      ...(status ? { status: statusForSelectedSnapshot(contact, status, incumbentSnapshot, {
        stale: Boolean(incumbentSnapshot), lastSuccessAt: incumbentStatus?.lastSuccessAt ?? status.lastSuccessAt ?? null,
      }) } : {}),
    };
  }
  if (!incumbentSnapshot) {
    return { snapshot: candidateSnapshot, status: statusForSelectedSnapshot(contact, candidateStatus, candidateSnapshot, { stale: false }) };
  }

  const incumbentRevision = incumbentSnapshot.profile.revision;
  const candidateRevision = candidateSnapshot.profile.revision;
  const sameDigest = incumbentSnapshot.source.rawSha256 === candidateSnapshot.source.rawSha256;
  if (candidateSnapshot.profile.identityIdBase64 !== incumbentSnapshot.profile.identityIdBase64) {
    const status = candidateIsLatest && candidateStatus
      ? statusForSelectedSnapshot(contact, candidateStatus, incumbentSnapshot, {
        stale: true, errorCode: "identity-metadata-changed", lastAttempt: "rejected",
        lastSuccessAt: incumbentStatus?.lastSuccessAt ?? null,
      })
      : statusForSelectedSnapshot(contact, incumbentStatus, incumbentSnapshot);
    return { snapshot: incumbentSnapshot, status };
  }
  if (candidateRevision > incumbentRevision) {
    return { snapshot: candidateSnapshot, status: statusForSelectedSnapshot(contact, candidateStatus, candidateSnapshot, { stale: false }) };
  }
  if (candidateRevision === incumbentRevision && sameDigest) {
    const useCandidate = compareAttempts(candidateStatus, incumbentStatus) > 0;
    const snapshot = useCandidate ? candidateSnapshot : incumbentSnapshot;
    const status = useCandidate ? candidateStatus : incumbentStatus;
    return { snapshot, status: statusForSelectedSnapshot(contact, status, snapshot, { stale: false }) };
  }

  const anomaly = candidateRevision < incumbentRevision ? "rollback" : "fork";
  const status = candidateIsLatest && candidateStatus
    ? statusForSelectedSnapshot(contact, candidateStatus, incumbentSnapshot, {
      stale: true, errorCode: anomaly, lastAttempt: "rejected", lastSuccessAt: incumbentStatus?.lastSuccessAt ?? null,
    })
    : statusForSelectedSnapshot(contact, incumbentStatus, incumbentSnapshot);
  return { snapshot: incumbentSnapshot, status };
}

export async function serializedContactProfileCommit(contact, candidate, adapter) {
  if (typeof adapter?.lock !== "function" || typeof adapter?.read !== "function" || typeof adapter?.write !== "function") {
    throw new ContactProfileError("Serialized profile commit adapter is invalid");
  }
  return adapter.lock(async () => {
    const incumbent = await adapter.read();
    const merged = await mergeContactProfileState(contact, incumbent, candidate);
    await adapter.write(merged);
    return merged;
  });
}

export function createContactProfileViewCache({ consumer, resolveContact }) {
  if (!consumer || typeof resolveContact !== "function") throw new ContactProfileError("Profile view adapter is invalid");
  for (const method of ["refresh", "current", "status", "renderCurrent"]) {
    if (typeof consumer[method] !== "function") throw new ContactProfileError("Profile view adapter is invalid");
  }
  const entries = new Map();
  const generations = new Map();
  const advance = (id) => {
    const generation = (generations.get(id) ?? 0) + 1;
    generations.set(id, generation);
    return generation;
  };
  const stillCurrent = (contact, fingerprint, generation) => {
    const configured = resolveContact(contact.contactId);
    return generations.get(contact.contactId) === generation && configured
      && contactRelationshipFingerprint(configured) === fingerprint;
  };

  const load = async (contact, refresh) => {
    const fingerprint = contactRelationshipFingerprint(contact);
    const generation = advance(contact.contactId);
    if (!stillCurrent(contact, fingerprint, generation)) return { discarded: true };
    entries.set(contact.contactId, { fingerprint, loading: true });
    let value;
    try {
      const result = refresh ? await consumer.refresh(contact) : {};
      const current = await consumer.current(contact);
      const status = await consumer.status(contact);
      const projection = await consumer.renderCurrent(contact, { includeBio: true });
      const statusProjection = typeof consumer.renderStatus === "function" ? await consumer.renderStatus(contact) : undefined;
      value = {
        fingerprint, ...(current && projection ? { current, projection } : {}), status,
        ...(statusProjection ? { statusProjection } : {}),
        ...(result.error ? { error: result.error } : {}),
      };
    } catch (error) {
      value = {
        fingerprint, error: `Profile adapter failed without changing messaging: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
    if (!stillCurrent(contact, fingerprint, generation)) return { discarded: true };
    entries.set(contact.contactId, value);
    return { discarded: false, value };
  };

  return Object.freeze({
    refresh(contact) { return load(contact, true); },
    reload(contact) { return load(contact, false); },
    read(contact) {
      const value = entries.get(contact.contactId);
      if (!value) return undefined;
      if (value.fingerprint !== contactRelationshipFingerprint(contact)) {
        advance(contact.contactId);
        entries.delete(contact.contactId);
        return undefined;
      }
      return value;
    },
    remove(id) {
      advance(id);
      entries.delete(id);
    },
  });
}

export function createMemoryContactProfiles({
  fetchCurrent, resolveContact, fetchTimeoutMs = 10_000,
  scheduleTimeout = setTimeout, cancelTimeout = clearTimeout,
  clock = () => new Date().toISOString(), randomId = () => crypto.randomUUID(),
}) {
  if (typeof fetchCurrent !== "function") throw new ContactProfileError("Profile fetch adapter is required");
  if (typeof resolveContact !== "function") throw new ContactProfileError("Authoritative contact resolver is required");
  if (!Number.isSafeInteger(fetchTimeoutMs) || fetchTimeoutMs < 1 || fetchTimeoutMs > 300_000
    || typeof scheduleTimeout !== "function" || typeof cancelTimeout !== "function") {
    throw new ContactProfileError("Profile fetch timeout configuration is invalid");
  }
  const records = new Map();
  const activeFingerprints = new Map();
  let tail = Promise.resolve();
  const lock = (operation) => {
    const result = tail.then(operation);
    tail = result.catch(() => {});
    return result;
  };
  const assertAuthoritative = (contact) => {
    const authoritative = resolveContact(contact.contactId);
    const fingerprint = contactRelationshipFingerprint(contact);
    if (!authoritative || contactRelationshipFingerprint(authoritative) !== fingerprint) {
      throw new ContactProfileError("Contact relationship is not authoritative");
    }
    const previous = activeFingerprints.get(contact.contactId);
    if (previous && previous !== fingerprint) records.delete(previous);
    activeFingerprints.set(contact.contactId, fingerprint);
    return fingerprint;
  };
  const key = (contact) => assertAuthoritative(contact);

  async function fetchBounded(contact, callerSignal) {
    const controller = new AbortController();
    let timeoutHandle;
    let removeCancellation = () => {};
    const timeout = new Promise((_, reject) => {
      timeoutHandle = scheduleTimeout(() => {
        reject(new ContactProfileTimeoutError());
        controller.abort();
      }, fetchTimeoutMs);
    });
    const cancellation = new Promise((_, reject) => {
      if (!callerSignal) return;
      const cancel = () => {
        controller.abort();
        reject(new DOMException("Profile fetch cancelled", "AbortError"));
      };
      if (callerSignal.aborted) cancel();
      else {
        callerSignal.addEventListener("abort", cancel, { once: true });
        removeCancellation = () => callerSignal.removeEventListener("abort", cancel);
      }
    });
    const fetch = Promise.resolve().then(() => fetchCurrent(contact, { signal: controller.signal }));
    fetch.catch(() => {});
    try { return await Promise.race([fetch, timeout, cancellation]); }
    finally { cancelTimeout(timeoutHandle); removeCancellation(); }
  }

  async function current(contact) {
    const record = records.get(key(contact));
    if (!record?.snapshot) return null;
    const value = await restoreContactSnapshot(contact, record.snapshot, record.status);
    assertAuthoritative(contact);
    return value;
  }

  return Object.freeze({
    async refresh(contact, options = {}) {
      const recordKey = key(contact);
      const attemptId = randomId();
      let candidate;
      let fetchError;
      try {
        const fetched = await fetchBounded(contact, options.signal);
        const snapshot = await contactSnapshot(contact, fetched);
        candidate = { snapshot, status: successfulContactStatus(contact, snapshot, undefined, clock(), attemptId) };
      } catch (error) {
        fetchError = error;
        candidate = { status: failedContactStatus(contact, undefined, undefined, error, clock(), attemptId) };
      }
      const merged = await serializedContactProfileCommit(contact, candidate, {
        lock,
        read: () => { assertAuthoritative(contact); return structuredClone(records.get(recordKey)); },
        write: (value) => { assertAuthoritative(contact); records.set(recordKey, structuredClone(value)); },
      });
      const value = await current(contact);
      const ownsLatestStatus = merged.status?.attemptId === attemptId;
      const error = ownsLatestStatus && (merged.status?.lastAttempt === "rejected" || fetchError)
        ? (merged.status?.errorCode || (fetchError instanceof Error ? fetchError.message : String(fetchError)))
        : undefined;
      return {
        kind: error ? (merged.status?.lastAttempt === "rejected" ? "rejected" : "unavailable")
          : candidate.snapshot && merged.snapshot?.source?.rawSha256 === candidate.snapshot.source.rawSha256 ? "accepted" : "confirmed",
        current: value, status: structuredClone(merged.status), ...(error ? { error } : {}),
      };
    },
    current,
    status(contact) {
      const record = records.get(key(contact));
      return structuredClone(validatedContactStatus(contact, record?.status, record?.snapshot));
    },
    renderStatus(contact) {
      const record = records.get(key(contact));
      const status = validatedContactStatus(contact, record?.status, record?.snapshot);
      if (!status) return null;
      return {
        version: 1,
        classification: "owner-local-profile-attempt-status",
        instructions: false,
        trustedForAuthority: false,
        remoteClaims: false,
        contactId: contact.contactId,
        attemptedAt: status.lastAttemptAt,
        outcome: status.lastAttempt,
        code: status.errorCode,
        currentRawSha256: status.snapshotRawSha256,
      };
    },
    async renderCurrent(contact, { includeBio = false } = {}) {
      const value = await current(contact);
      if (!value) return null;
      assertAuthoritative(contact);
      return {
        version: 1,
        classification: "untrusted-owner-authored-descriptive-metadata",
        instructions: false,
        trustedForAuthority: false,
        source: {
          qualified: canonicalEndpoint(contact),
          path: ["profile.scm"], view: "current-get", observedAt: value.observedAt,
          authenticationScope: value.authenticationScope,
          terminalJournalContinuity: value.terminalJournalContinuity,
          stale: value.stale, rawSha256: value.rawSha256,
        },
        profile: { ...value.profile, bio: includeBio ? value.profile.bio : null },
      };
    },
  });
}
