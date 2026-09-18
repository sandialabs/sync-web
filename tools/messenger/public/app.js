import { GatewayError, MessengerApi } from "./api.mjs";
import { profileProvenanceLabel } from "./profile.mjs";
import { createContactProfileViewCache, createMemoryContactProfiles } from "./contact-profile.mjs";
import {
  assertContactRemovable, contactRegistryDocument, conversationIdForContact, conversationIdForGroup, findReplyParent, mergeMessage,
  migrateRouteCanonicalRegistry, normalizeContacts, persistedMessages, readyGroupTargets, replyExcerpt, replyReferenceFor, sortConversations,
} from "./logic.mjs";

const config = window.MESSENGER_CONFIG;
const api = new MessengerApi(config);
const STORAGE_KEY = "sync-messenger-v1";
const LEGACY_STORAGE_KEY = "sync-messenger-prototype-v1";
const THEME_KEY = "sync-messenger-theme";

const elements = Object.fromEntries([
  "app", "connection-pill", "notify-button", "theme-button", "account-button", "nav-unread", "list-title", "new-group-button",
  "search-input", "conversation-list", "contacts-view", "groups-view", "profile-view", "empty-state", "detail-view", "conversation",
  "conversation-avatar", "conversation-name", "conversation-subtitle", "block-button", "mobile-back", "message-list", "composer",
  "reply-preview", "reply-label", "reply-excerpt", "cancel-reply", "message-input", "send-button",
  "group-dialog", "group-form", "group-name", "group-contact-options", "toast-region",
  "auth-screen", "auth-message", "login-button", "contact-dialog", "contact-form",
  "contact-handle", "contact-identity", "contact-journal", "contact-owner", "contact-route", "contact-principal",
].map((id) => [id.replaceAll("-", "_"), document.getElementById(id)]));

const state = {
  seedContacts: [],
  contacts: [],
  grants: {},
  groups: [],
  messages: [],
  readAt: {},
  blocked: [],
  quarantine: {},
  selectedId: undefined,
  selectedContactId: undefined,
  selectedGroupId: undefined,
  replyTarget: undefined,
  view: "messages",
  identity: undefined,
  localOwner: undefined,
  localJournal: undefined,
  localIdentityIdBase64: undefined,
  localProfile: undefined,
  localProfileError: undefined,
  contactProfileViews: undefined,
  profileMigration: { ready: true, migrated: false },
  pollTimer: undefined,
  polling: false,
  hasPolled: false,
  sending: false,
  initializedPeers: new Set(),
  recoveredOutgoingContacts: new Set(),
  authPopup: undefined,
  sessionTimer: undefined,
};

function accountStorageKey(prefix = STORAGE_KEY) {
  const username = state.identity?.identity?.traits?.username;
  return username && state.localJournal ? `${prefix}:${location.origin}:${state.localJournal}:${username}` : undefined;
}

function restore() {
  const key = accountStorageKey();
  state.groups = []; state.messages = []; state.readAt = {}; state.blocked = []; state.quarantine = {};
  state.recoveredOutgoingContacts.clear();
  if (!key) return;
  try {
    const legacyKey = accountStorageKey(LEGACY_STORAGE_KEY);
    const current = localStorage.getItem(key);
    const legacy = current === null && legacyKey ? localStorage.getItem(legacyKey) : null;
    const value = JSON.parse(current ?? legacy ?? "{}");
    if (legacy !== null) {
      localStorage.setItem(key, legacy);
      localStorage.removeItem(legacyKey);
    }
    state.groups = Array.isArray(value.groups) ? value.groups : [];
    state.messages = persistedMessages([], Array.isArray(value.messages) ? value.messages : []);
    state.readAt = value.readAt && typeof value.readAt === "object" ? value.readAt : {};
    state.blocked = Array.isArray(value.blocked) ? value.blocked : [];
    state.quarantine = value.quarantine && typeof value.quarantine === "object" ? value.quarantine : {};
  } catch {
    localStorage.removeItem(key);
    const legacyKey = accountStorageKey(LEGACY_STORAGE_KEY);
    if (legacyKey) localStorage.removeItem(legacyKey);
  }
}

function persist() {
  const key = accountStorageKey();
  if (!key) return;
  let stored = {};
  try { stored = JSON.parse(localStorage.getItem(key) || "{}"); }
  catch { /* The current validated state replaces malformed local data. */ }
  const groups = new Map();
  for (const group of [...(Array.isArray(stored.groups) ? stored.groups : []), ...state.groups]) groups.set(group.id, group);
  const messages = persistedMessages(Array.isArray(stored.messages) ? stored.messages : [], state.messages);
  const readAt = { ...(stored.readAt && typeof stored.readAt === "object" ? stored.readAt : {}), ...state.readAt };
  state.groups = [...groups.values()];
  state.messages = messages;
  localStorage.setItem(key, JSON.stringify({
    groups: state.groups, messages, readAt, blocked: state.blocked, quarantine: state.quarantine,
  }));
}

function setConnection(kind, text) {
  const label = elements.connection_pill.querySelector("span");
  if (elements.connection_pill.className === `status-pill ${kind}` && label.textContent === text) return;
  elements.connection_pill.className = `status-pill ${kind}`;
  label.textContent = text;
}

function toast(message, timeout = 4500) {
  const node = document.createElement("div");
  node.className = "toast";
  node.textContent = message;
  elements.toast_region.append(node);
  setTimeout(() => node.remove(), timeout);
}

function initials(name) {
  return name.split(/\s+/).map((part) => part[0]).join("").slice(0, 2).toUpperCase();
}

function formatTime(value) {
  const date = new Date(value);
  const now = new Date();
  if (date.toDateString() === now.toDateString()) return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return date.toLocaleDateString([], { month: "short", day: "numeric" });
}

function conversations() {
  const contacts = state.contacts.map((contact) => ({
    id: conversationIdForContact(contact.id), type: "contact", ref: contact.id, name: contact.handle,
    subtitle: `${contact.identity}@${contact.journal}`, color: contact.color,
  }));
  const groups = state.groups.map((group) => ({
    id: conversationIdForGroup(group.id), type: "group", ref: group.id, name: group.name,
    subtitle: `${group.contactIds.length} recipients`, color: "#e05a3f",
  }));
  return sortConversations([...contacts, ...groups], state.messages);
}

function selectedConversation() { return conversations().find((item) => item.id === state.selectedId); }
function messagesFor(id) { return state.messages.filter((message) => message.conversationId === id).sort((a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt)); }

function contactEndpoint(contact) { return `${contact.identity}@${contact.journal}`; }

function senderLabel(message) {
  if (message.from === `${state.localOwner}@${state.localJournal}`) return "You";
  return state.contacts.find((contact) => contactEndpoint(contact) === message.from)?.handle ?? message.from ?? "Unknown sender";
}

function renderComposerReply() {
  const target = state.replyTarget;
  const visible = target?.conversationId === state.selectedId;
  elements.reply_preview.hidden = !visible;
  if (!visible) return;
  elements.reply_label.textContent = `Replying to ${senderLabel(target)}`;
  elements.reply_excerpt.textContent = replyExcerpt(target.body) || "Message has no preview";
}

function chooseReply(message) {
  state.replyTarget = {
    conversationId: message.conversationId,
    id: message.id,
    from: message.from,
    body: message.body,
  };
  renderComposerReply();
  elements.message_input.focus();
}

function clearReply() {
  state.replyTarget = undefined;
  renderComposerReply();
}

function scrollToMessage(sourceKey) {
  const row = [...elements.message_list.querySelectorAll(".message")].find((node) => node.dataset.sourceKey === sourceKey);
  if (!row) return;
  row.scrollIntoView({ block: "center", behavior: "smooth" });
  row.classList.remove("reply-highlight");
  requestAnimationFrame(() => row.classList.add("reply-highlight"));
  setTimeout(() => row.classList.remove("reply-highlight"), 1400);
}

function participantsForGroup(group) {
  return [`${state.localOwner}@${state.localJournal}`, ...group.contactIds.map((id) => {
    const contact = state.contacts.find((item) => item.id === id);
    if (!contact) throw new Error(`Group contact is missing: ${id}`);
    return contactEndpoint(contact);
  })].sort();
}

function ensureIncomingGroup(envelope) {
  const participantContacts = envelope.participants
    .filter((participant) => participant !== `${state.localOwner}@${state.localJournal}`)
    .map((participant) => state.contacts.find((contact) => contactEndpoint(contact) === participant));
  if (participantContacts.some((contact) => !contact)) throw new Error("Group message includes an unknown contact");
  const existing = state.groups.find((group) => group.id === envelope.conversationId);
  if (existing) {
    if (participantsForGroup(existing).join("\0") !== envelope.participants.join("\0")) throw new Error("Group membership changed for an existing conversation");
    return existing;
  }
  const contactIds = participantContacts.map((contact) => contact.id);
  const group = {
    id: envelope.conversationId,
    name: participantContacts.map((contact) => contact.handle).join(", "),
    contactIds,
    createdAt: envelope.createdAt,
  };
  state.groups.push(group);
  return group;
}
function unreadFor(id) {
  const read = Date.parse(state.readAt[id] || 0) || 0;
  return state.messages.filter((message) => message.conversationId === id && message.direction === "inbound" && Date.parse(message.observedAt || message.createdAt) > read).length;
}

function avatar(name, color, group = false) {
  const node = document.createElement("div");
  node.className = `avatar${group ? " group" : ""}`;
  node.style.setProperty("--avatar", color || "#245b66");
  node.textContent = group ? "◌" : initials(name);
  return node;
}

function renderNavigation() {
  const total = conversations().reduce((sum, item) => sum + unreadFor(item.id), 0);
  elements.nav_unread.hidden = total === 0;
  elements.nav_unread.textContent = total > 99 ? "99+" : String(total);
  document.querySelectorAll(".rail-button").forEach((button) => button.classList.toggle("active", button.dataset.view === state.view));
}

function renderConversationList() {
  const query = elements.search_input.value.trim().toLowerCase();
  elements.conversation_list.replaceChildren();
  const rows = conversations().filter((item) => `${item.name} ${item.subtitle}`.toLowerCase().includes(query));
  for (const item of rows) {
    const latest = messagesFor(item.id).at(-1);
    const unread = unreadFor(item.id);
    const row = document.createElement("button");
    row.type = "button";
    row.className = `conversation-row${state.selectedId === item.id ? " selected" : ""}`;
    row.append(avatar(item.name, item.color, item.type === "group"));
    const copy = document.createElement("div");
    copy.className = "conversation-copy";
    const title = document.createElement("strong"); title.textContent = item.name;
    const preview = document.createElement("span"); preview.textContent = latest ? latest.body.replace(/\s+/g, " ") : item.subtitle;
    copy.append(title, preview);
    const meta = document.createElement("div"); meta.className = "conversation-meta";
    if (latest) { const time = document.createElement("span"); time.textContent = formatTime(latest.observedAt || latest.createdAt); meta.append(time); }
    if (unread) { const badge = document.createElement("b"); badge.className = "unread-dot"; badge.textContent = unread > 9 ? "9+" : String(unread); meta.append(badge); }
    row.append(copy, meta);
    row.addEventListener("click", () => openConversation(item.id));
    elements.conversation_list.append(row);
  }
}

function contactGrantLabel(contact) {
  const quarantineCount = Object.values(state.quarantine).filter((entry) => entry.contactId === contact.id).length;
  if (state.blocked.includes(contact.id)) return "BLOCKED";
  if (state.grants[contact.id] === "grant-ready") return quarantineCount ? `GRANT READY · QUARANTINE ${quarantineCount}` : "GRANT READY";
  return state.grants[contact.id] === "error" ? "GRANT ERROR" : "GRANT PENDING";
}

function renderContacts() {
  elements.contacts_view.replaceChildren();
  for (const contact of state.contacts) {
    const card = document.createElement("div");
    card.className = `secondary-card contact-card${state.selectedContactId === contact.id ? " selected" : ""}`;
    const select = document.createElement("button"); select.type = "button"; select.className = "contact-select";
    select.append(avatar(contact.handle, contact.color));
    const copy = document.createElement("div"); copy.className = "contact-summary";
    const title = document.createElement("h3"); title.textContent = contact.handle;
    const path = document.createElement("code"); path.textContent = `${contact.identity}@${contact.journal} · ${contact.route.join("/")}`;
    copy.append(title, path); select.append(copy);
    const status = document.createElement("span"); status.className = "contact-state"; status.textContent = contactGrantLabel(contact); select.append(status);
    select.addEventListener("click", () => {
      state.selectedContactId = contact.id;
      document.body.classList.add("conversation-open");
      renderView();
    });
    const remove = document.createElement("button"); remove.type = "button"; remove.className = "contact-remove quiet-button"; remove.textContent = "Remove";
    remove.addEventListener("click", () => removeContact(contact));
    card.append(select, remove); elements.contacts_view.append(card);
  }
  const stub = document.createElement("div"); stub.className = "stub-card";
  const copy = document.createElement("p"); copy.textContent = "Contacts keep outbound profile routes separate from incoming mailbox grants.";
  const add = document.createElement("button"); add.id = "contact-add-button"; add.type = "button"; add.className = "primary-button"; add.textContent = "Add contact";
  add.addEventListener("click", openContactDialog); stub.append(copy, add); elements.contacts_view.append(stub);
}

function renderGroups() {
  elements.groups_view.replaceChildren();
  for (const group of state.groups) {
    const card = document.createElement("button"); card.type = "button";
    card.className = `secondary-card conversation-row${state.selectedGroupId === group.id ? " selected" : ""}`;
    card.append(avatar(group.name, "#e05a3f", true));
    const copy = document.createElement("div"); copy.className = "conversation-copy";
    const title = document.createElement("strong"); title.textContent = group.name;
    const members = document.createElement("span");
    members.textContent = group.contactIds.map((contactId) => state.contacts.find((contact) => contact.contactId === contactId)?.handle).filter(Boolean).join(", ");
    copy.append(title, members); card.append(copy);
    card.addEventListener("click", () => {
      state.selectedGroupId = group.id;
      document.body.classList.add("conversation-open");
      renderView();
    });
    elements.groups_view.append(card);
  }
  if (!state.groups.length) {
    const stub = document.createElement("div"); stub.className = "stub-card";
    stub.innerHTML = "<strong>No groups yet</strong>Groups are fixed local membership lists with independent point-to-point fanout.";
    elements.groups_view.append(stub);
  }
}

function renderProfile() {
  elements.profile_view.replaceChildren();
  const card = document.createElement("div"); card.className = "stub-card profile-sidebar-card";
  const heading = document.createElement("strong"); heading.textContent = "Your public profile";
  const address = document.createElement("code");
  address.textContent = state.localOwner && state.localJournal ? `${state.localOwner}@${state.localJournal}` : "Authenticated Journal";
  const copy = document.createElement("p"); copy.textContent = "View and edit your owner-authored Profile document in the main pane.";
  card.append(heading, address, copy); elements.profile_view.append(card);
}

function setMainEmpty(mark, eyebrow, title, description) {
  elements.conversation.hidden = true;
  elements.detail_view.hidden = true;
  elements.empty_state.hidden = false;
  elements.empty_state.querySelector(".empty-mark").textContent = mark;
  elements.empty_state.querySelector(".eyebrow").textContent = eyebrow;
  elements.empty_state.querySelector("h2").textContent = title;
  elements.empty_state.querySelector("p:last-child").textContent = description;
}

function showDetail() {
  elements.empty_state.hidden = true;
  elements.conversation.hidden = true;
  elements.detail_view.hidden = false;
  elements.detail_view.replaceChildren();
  return elements.detail_view;
}

function detailHeader({ eyebrow, title, subtitle, mark, action }) {
  const header = document.createElement("header"); header.className = "detail-header";
  const back = document.createElement("button"); back.type = "button"; back.className = "icon-button mobile-only"; back.textContent = "←"; back.ariaLabel = "Back";
  back.addEventListener("click", () => document.body.classList.remove("conversation-open"));
  const icon = document.createElement("div"); icon.className = "detail-mark"; icon.textContent = mark;
  const copy = document.createElement("div");
  const overline = document.createElement("p"); overline.className = "eyebrow"; overline.textContent = eyebrow;
  const heading = document.createElement("h2"); heading.textContent = title;
  const sub = document.createElement("p"); sub.textContent = subtitle;
  copy.append(overline, heading, sub); header.append(back, icon, copy);
  if (action) header.append(action);
  return header;
}

function renderContactDetail() {
  const contact = state.contacts.find((item) => item.id === state.selectedContactId);
  if (!contact) {
    setMainEmpty("◎", "Configured relationships", "Choose a contact", "Select a contact to view or explicitly refresh their public profile.");
    return;
  }
  const root = showDetail();
  const refresh = document.createElement("button"); refresh.type = "button"; refresh.className = "primary-button";
  const observed = state.contactProfileViews?.read(contact);
  refresh.textContent = observed?.loading ? "Refreshing…" : observed?.current ? "Refresh profile" : "Fetch profile";
  refresh.disabled = !state.profileMigration.ready || observed?.loading === true;
  refresh.addEventListener("click", async () => {
    refresh.disabled = true; refresh.textContent = "Refreshing…";
    await refreshContactProfile(contact);
  });
  root.append(detailHeader({
    eyebrow: "Contact profile", title: contact.handle,
    subtitle: `${contactEndpoint(contact)} · route ${contact.route.join("/")}`,
    mark: initials(contact.handle), action: refresh,
  }));
  const relationship = document.createElement("section"); relationship.className = "detail-card relationship-card";
  const relTitle = document.createElement("h3"); relTitle.textContent = "Configured relationship";
  const relGrid = document.createElement("dl"); relGrid.className = "fact-grid";
  for (const [label, value] of [
    ["Profile owner", contact.owner], ["Journal", contact.journal], ["Profile path", "profile.scm"],
    ["Incoming mailbox principal", contact.incomingPrincipal.join(" / ")], ["Messaging", contactGrantLabel(contact)],
  ]) {
    const dt = document.createElement("dt"); dt.textContent = label;
    const dd = document.createElement("dd"); dd.textContent = value; relGrid.append(dt, dd);
  }
  relationship.append(relTitle, relGrid); root.append(relationship);
  if (!observed?.current || !observed.projection) {
    const empty = document.createElement("section"); empty.className = "detail-card profile-empty-card";
    const title = document.createElement("h3"); title.textContent = observed?.error ? "Profile unavailable" : "No profile fetched this session";
    const text = document.createElement("p");
    text.textContent = observed?.error ?? "Profile refresh is explicit. Messaging remains available whether or not a profile can be read.";
    empty.append(title, text);
    if (observed?.statusProjection) {
      const attempt = document.createElement("small");
      attempt.textContent = `Latest attempt: ${observed.statusProjection.outcome} · ${observed.statusProjection.code ?? "no code"} · ${observed.statusProjection.attemptedAt}`;
      empty.append(attempt);
    }
    root.append(empty); return;
  }
  const projection = observed.projection;
  const profileCard = document.createElement("section"); profileCard.className = "detail-card remote-profile-card";
  const name = document.createElement("h3"); name.textContent = projection.profile.displayName;
  const pronouns = document.createElement("p"); pronouns.className = "profile-pronouns"; pronouns.textContent = projection.profile.pronouns?.join(" / ") ?? "No pronouns listed";
  const bio = document.createElement("p"); bio.className = "profile-bio"; bio.textContent = projection.profile.bio ?? "No bio published.";
  const facts = document.createElement("dl"); facts.className = "fact-grid profile-facts";
  for (const [label, value] of [
    ["Revision", String(projection.profile.revision)], ["Freshness", projection.source.stale ? "Stale after failed refresh" : "Fresh at observation"],
    ["Observed", projection.source.observedAt], ["SHA-256", projection.source.rawSha256],
  ]) {
    const dt = document.createElement("dt"); dt.textContent = label;
    const dd = document.createElement("dd"); dd.textContent = value; facts.append(dt, dd);
  }
  const provenance = document.createElement("p"); provenance.className = "profile-provenance detail-provenance";
  provenance.textContent = profileProvenanceLabel(contactEndpoint(contact), observed.current);
  profileCard.append(name, pronouns, bio, facts, provenance); root.append(profileCard);
}

function renderGroupDetail() {
  const group = state.groups.find((item) => item.id === state.selectedGroupId);
  if (!group) {
    setMainEmpty("◌", "Fixed fanout", "Choose a group", "Select a group to inspect its participants and messaging readiness.");
    return;
  }
  const root = showDetail();
  const open = document.createElement("button"); open.type = "button"; open.className = "primary-button"; open.textContent = "Open conversation";
  open.addEventListener("click", () => openConversation(conversationIdForGroup(group.id)));
  root.append(detailHeader({ eyebrow: "Fixed group", title: group.name, subtitle: `${group.contactIds.length} recipients · independent fanout`, mark: "◌", action: open }));
  const card = document.createElement("section"); card.className = "detail-card";
  const heading = document.createElement("h3"); heading.textContent = "Participants"; card.append(heading);
  const list = document.createElement("div"); list.className = "participant-list";
  for (const contactId of group.contactIds) {
    const contact = state.contacts.find((item) => item.id === contactId);
    const row = document.createElement("div"); row.className = "participant-row";
    if (contact) {
      row.append(avatar(contact.handle, contact.color));
      const copy = document.createElement("div");
      const name = document.createElement("strong"); name.textContent = contact.handle;
      const endpoint = document.createElement("span"); endpoint.textContent = contactEndpoint(contact);
      copy.append(name, endpoint); row.append(copy);
      const status = document.createElement("b"); status.textContent = contactGrantLabel(contact); row.append(status);
    } else {
      row.textContent = `${contactId} · contact no longer configured`;
    }
    list.append(row);
  }
  card.append(list); root.append(card);
}

function renderLocalProfileDetail() {
  const root = showDetail();
  root.append(detailHeader({
    eyebrow: "Local publisher", title: "Your public profile",
    subtitle: state.localOwner && state.localJournal ? `${state.localOwner}@${state.localJournal}` : "Authenticated Journal",
    mark: "◇",
  }));
  const card = document.createElement("section"); card.className = "detail-card local-profile-card";
  if (state.localProfileError) {
    const error = document.createElement("p"); error.className = "profile-error"; error.textContent = state.localProfileError; card.append(error);
  }
  if (!state.localProfile) {
    const unavailable = document.createElement("p"); unavailable.textContent = "Local profile state could not be read. Messaging remains available.";
    card.append(unavailable); root.append(card); return;
  }
  const current = state.localProfile.profile;
  const meta = document.createElement("p"); meta.className = "profile-meta";
  meta.textContent = current
    ? `Validated current profile · schema v${current.schemaVersion} · revision ${current.revision} · observed ${formatTime(state.localProfile.observedAt)}`
    : "No profile document exists yet. Your first save will create Profile v1 revision 1 with an exact create-only comparison.";
  const form = document.createElement("form"); form.className = "profile-form";
  const display = document.createElement("input"); display.name = "displayName"; display.maxLength = 128; display.required = true; display.value = current?.displayName ?? "";
  const pronouns = document.createElement("input"); pronouns.name = "pronouns"; pronouns.placeholder = "he/him, they/them"; pronouns.value = current?.pronouns?.join(", ") ?? "";
  const bio = document.createElement("textarea"); bio.name = "bio"; bio.maxLength = 2048; bio.rows = 9; bio.required = true; bio.value = current?.bio ?? "";
  const displayLabel = document.createElement("label"); displayLabel.className = "field-label"; displayLabel.append("Display name", display);
  const pronounLabel = document.createElement("label"); pronounLabel.className = "field-label"; pronounLabel.append("Pronouns (up to four, comma separated)", pronouns);
  const bioLabel = document.createElement("label"); bioLabel.className = "field-label"; bioLabel.append("Bio", bio);
  const note = document.createElement("p"); note.className = "dialog-note";
  note.textContent = "Public descriptive metadata only. Saving uses one exact compare-and-set and validates exact current readback; it never retries.";
  const save = document.createElement("button"); save.type = "submit"; save.className = "primary-button";
  save.textContent = current == null ? "Create profile revision 1" : current.schemaVersion === 0 ? "Migrate and save revision 1" : `Save revision ${current.revision + 1}`;
  form.append(displayLabel, pronounLabel, bioLabel, note, save);
  form.addEventListener("submit", saveProfile); card.append(meta, form); root.append(card);
}

async function refreshLocalProfile() {
  state.localProfileError = undefined;
  if (!state.localIdentityIdBase64) {
    state.localProfile = undefined;
    state.localProfileError = "Profile unbound: Journal info did not provide a valid 32-byte identity ID. Messaging remains available.";
    if (state.view === "profile") renderLocalProfileDetail();
    return;
  }
  try {
    state.localProfile = await api.readLocalProfile({ owner: state.localOwner, journal: state.localJournal, identityIdBase64: state.localIdentityIdBase64 });
  } catch (error) {
    state.localProfile = undefined;
    state.localProfileError = error instanceof Error ? error.message : String(error);
  }
  if (state.view === "profile") renderLocalProfileDetail();
}

async function saveProfile(event) {
  event.preventDefault();
  const button = event.currentTarget.querySelector('button[type="submit"]'); button.disabled = true;
  const fields = new FormData(event.currentTarget);
  try {
    const result = await api.saveLocalProfile(state.localProfile, {
      displayName: fields.get("displayName"),
      pronouns: String(fields.get("pronouns") || "").split(",").map((value) => value.trim()).filter(Boolean),
      bio: fields.get("bio"),
    });
    if (result.status === "conflict") {
      toast("Profile changed in another session. Current bytes were not overwritten; review the refreshed profile.", 7500);
      await refreshLocalProfile(); return;
    }
    state.localProfile = result.current; state.localProfileError = undefined; renderView();
    toast("Profile saved as current state after exact readback. Historical commitment is separate.", 6500);
  } catch (error) {
    const attemptedBase = state.localProfile;
    const dispatched = api.isProfileEditBaseConsumed(attemptedBase);
    if (dispatched) {
      state.localProfile = undefined;
      await refreshLocalProfile();
    }
    const prefix = dispatched
      ? "Profile save failed or is ambiguous; current state was refreshed before another edit"
      : "Profile save was not dispatched";
    toast(`${prefix}: ${error instanceof Error ? error.message : String(error)}`, 8000);
  } finally { button.disabled = false; }
}

async function refreshContactProfile(contact, { render = true } = {}) {
  if (!state.profileMigration.ready || !state.contactProfileViews) return;
  const pending = state.contactProfileViews.refresh(contact);
  if (render) renderView();
  await pending;
  if (render) renderView();
}

function renderConversationPane() {
  const item = selectedConversation();
  if (!item) {
    setMainEmpty("✦", "Private point-to-point rails", "Choose a conversation", "Messages are delivered independently to each recipient over existing Sync Web routes.");
    return;
  }
  elements.empty_state.hidden = true;
  elements.detail_view.hidden = true;
  elements.conversation.hidden = false;
  elements.conversation_name.textContent = item.name;
  elements.conversation_subtitle.textContent = item.type === "group"
    ? `${item.subtitle} · point-to-point fanout`
    : `${item.subtitle} · configured contact`;
  elements.conversation_avatar.replaceWith(avatar(item.name, item.color, item.type === "group"));
  const replacement = document.querySelector(".conversation-header .avatar");
  replacement.id = "conversation-avatar"; elements.conversation_avatar = replacement;
  const blocked = item.type === "contact" && state.blocked.includes(item.ref);
  elements.block_button.hidden = item.type !== "contact";
  elements.block_button.textContent = blocked ? "Unblock" : "Block";
  elements.message_input.disabled = blocked;
  elements.message_input.placeholder = blocked ? "This contact is locally blocked" : "Write a message…";
  renderMessages();
  renderComposerReply();
}

function renderMainPane() {
  if (state.view === "messages") renderConversationPane();
  else if (state.view === "contacts") renderContactDetail();
  else if (state.view === "groups") renderGroupDetail();
  else renderLocalProfileDetail();
}

function renderView() {
  const map = { messages: elements.conversation_list, contacts: elements.contacts_view, groups: elements.groups_view, profile: elements.profile_view };
  Object.entries(map).forEach(([name, node]) => { node.hidden = state.view !== name; });
  const labels = { messages: "Conversations", contacts: "Contacts", groups: "Groups", profile: "Your profile" };
  elements.list_title.textContent = labels[state.view];
  elements.search_input.parentElement.hidden = state.view !== "messages";
  elements.new_group_button.hidden = !["messages", "groups"].includes(state.view);
  renderNavigation();
  renderConversationList();
  renderContacts();
  renderGroups();
  renderProfile();
  renderMainPane();
}

function renderMessages() {
  elements.message_list.replaceChildren();
  const selected = selectedConversation();
  if (!selected) return;
  const projected = messagesFor(selected.id);
  for (const message of projected) {
    const row = document.createElement("article"); row.className = `message ${message.direction === "outbound" ? "outbound" : "inbound"}`;
    row.dataset.sourceKey = message.sourceKey ?? `${message.from}/${message.id}`;
    const bubble = document.createElement("div"); bubble.className = "message-bubble";
    if (message.inReplyTo) {
      const parent = findReplyParent(projected, message);
      const context = document.createElement(parent ? "button" : "div");
      context.className = `reply-context${parent ? "" : " unavailable"}`;
      if (parent) context.type = "button";
      const label = document.createElement("strong");
      label.textContent = parent ? `Reply to ${senderLabel(parent)}` : "Referenced message unavailable";
      const excerpt = document.createElement("span");
      const referenceId = typeof message.inReplyTo === "string" ? message.inReplyTo : message.inReplyTo.id;
      excerpt.textContent = parent ? (replyExcerpt(parent.body) || "Message has no preview") : `Message ${referenceId.slice(0, 8)}`;
      context.append(label, excerpt);
      if (parent) context.addEventListener("click", () => scrollToMessage(parent.sourceKey ?? `${parent.from}/${parent.id}`));
      bubble.append(context);
    }
    const body = document.createElement("div"); body.className = "message-body"; body.textContent = message.body;
    bubble.append(body);
    const meta = document.createElement("div"); meta.className = "message-meta";
    const time = document.createElement("time"); time.dateTime = message.createdAt; time.textContent = formatTime(message.createdAt);
    const status = document.createElement("span"); status.className = `message-status ${message.status || ""}`;
    if (message.direction === "outbound") {
      const outcomes = message.outcomes ? Object.values(message.outcomes) : [];
      const accepted = outcomes.filter((value) => value === "write-accepted").length;
      status.classList.toggle("accepted", accepted === outcomes.length && outcomes.length > 0);
      status.classList.toggle("error", outcomes.some((value) => value === "definite-failure" || value === "ambiguous"));
      status.textContent = outcomes.length
        ? `${accepted}/${outcomes.length} write accepted`
        : message.status === "sent-copy-observed" ? "sent copy recovered" : (message.status || "prepared");
    } else {
      status.textContent = selected.type === "group" ? `${senderLabel(message)} · validated source` : "validated source";
    }
    const reply = document.createElement("button"); reply.type = "button"; reply.className = "message-reply"; reply.textContent = "Reply";
    reply.setAttribute("aria-label", `Reply to ${senderLabel(message)}`);
    reply.addEventListener("click", () => chooseReply(message));
    meta.append(time, status, reply); row.append(bubble, meta); elements.message_list.append(row);
  }
  requestAnimationFrame(() => { elements.message_list.scrollTop = elements.message_list.scrollHeight; });
}

function openConversation(id) {
  if (state.replyTarget?.conversationId !== id) state.replyTarget = undefined;
  state.selectedId = id;
  state.view = "messages";
  state.readAt[id] = new Date().toISOString();
  persist();
  if (!selectedConversation()) return;
  document.body.classList.add("conversation-open");
  renderView();
}

function addLocalMessage(message) {
  state.messages = mergeMessage(state.messages, { ...message, updatedAt: new Date().toISOString() });
  persist(); renderView(); renderMessages();
}

async function mapBounded(items, limit, worker) {
  let cursor = 0;
  const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (cursor < items.length) {
      const index = cursor++;
      await worker(items[index], index);
    }
  });
  await Promise.all(runners);
}

function activeGrantContactId(selected) {
  return selected?.type === "contact" ? selected.ref : undefined;
}

async function sendCurrent(body) {
  const selected = selectedConversation();
  if (!selected) return;
  if (!state.identity) throw new Error("Kratos session unavailable");
  if (state.grants[activeGrantContactId(selected)] !== "grant-ready" && selected.type === "contact") throw new Error("Contact authorization is not ready");
  const sessionUser = state.identity?.identity?.traits?.username;
  if (sessionUser !== state.localOwner) throw new Error("Kratos session identity changed; sign in again");
  const operationId = crypto.randomUUID();
  const group = selected.type === "group" ? state.groups.find((item) => item.id === selected.ref) : undefined;
  const targets = group
    ? readyGroupTargets(group, state.contacts, state.blocked, state.grants)
    : [state.contacts.find((contact) => contact.id === selected.ref)];
  const activeTargets = targets.filter((contact) => contact && !state.blocked.includes(contact.id));
  if (!activeTargets.length) throw new Error("No unblocked contacts are available for this conversation.");
  if (activeTargets.some((contact) => state.grants[contact.id] !== "grant-ready")) throw new Error("Every recipient must have an exact incoming grant before sending");
  const createdAt = new Date().toISOString();
  const parent = state.replyTarget?.conversationId === selected.id ? state.replyTarget : undefined;
  const inReplyTo = parent ? replyReferenceFor(parent, Boolean(group)) : undefined;
  const messageOptions = group ? {
    conversationId: group.id,
    participants: participantsForGroup(group),
    ...(inReplyTo ? { inReplyTo } : {}),
  } : inReplyTo ? { inReplyTo } : {};
  const prepared = Object.fromEntries(activeTargets.map((contact) => {
    const value = api.prepare(contact, body, { id: group ? operationId : crypto.randomUUID(), createdAt, ...messageOptions });
    return [contact.id, value];
  }));
  const localId = group ? operationId : prepared[activeTargets[0].id].envelope.id;
  const local = {
    sourceKey: `outbound/${localId}`, id: localId, operationId, conversationId: selected.id,
    direction: "outbound", body, createdAt, observedAt: createdAt, status: "prepared", wireVersion: group ? 2 : 1,
    from: `${state.localOwner}@${state.localJournal}`, inReplyTo,
    memberSnapshot: activeTargets.map((contact) => contact.id),
    outcomes: Object.fromEntries(activeTargets.map((contact) => [contact.id, "not-attempted"])),
    prepared: Object.fromEntries(Object.entries(prepared).map(([contactId, value]) => [contactId, {
      id: value.envelope.id,
      contentHex: value.contentHex,
      route: value.request.$federation.route,
      path: value.request.path,
    }])),
  };
  state.replyTarget = undefined;
  addLocalMessage(local);
  await mapBounded(activeTargets, 3, async (contact) => {
    local.outcomes[contact.id] = "sending";
    local.status = "sending";
    addLocalMessage({ ...local });
    try {
      await api.sendPrepared(prepared[contact.id]);
      local.outcomes[contact.id] = "write-accepted";
    } catch (error) {
      local.outcomes[contact.id] = error instanceof GatewayError && error.status >= 400 && error.status < 500
        ? "definite-failure"
        : "ambiguous";
      local.errors = { ...(local.errors || {}), [contact.id]: error instanceof Error ? error.message : String(error) };
    }
    const outcomes = Object.values(local.outcomes);
    local.status = outcomes.every((value) => value === "write-accepted")
      ? "write-accepted"
      : outcomes.some((value) => value === "sending" || value === "not-attempted") ? "sending" : "partial";
    addLocalMessage({ ...local });
  });
  if (local.status !== "write-accepted") toast("Some recipients have definite or ambiguous outcomes. No automatic retry was attempted.", 6500);
}

async function recoverOutgoingContact(contact) {
  if (state.recoveredOutgoingContacts.has(contact.id)) return 0;
  const entries = await api.listOutgoingCopies(contact);
  let added = 0;
  for (const entry of entries) {
    const sourceKey = `outbound/${entry.name.toLowerCase()}`;
    if (state.messages.some((message) => message.sourceKey === sourceKey)) continue;
    const recovered = await api.readOutgoingCopy(contact, entry.name);
    const conversationId = recovered.envelope.version === 2
      ? conversationIdForGroup(ensureIncomingGroup(recovered.envelope).id)
      : conversationIdForContact(contact.id);
    const legacy = state.messages.find((message) => message.direction === "outbound"
      && message.conversationId === conversationId
      && message.body === recovered.envelope.body
      && message.createdAt === recovered.envelope.createdAt);
    if (legacy) state.messages = state.messages.filter((message) => message.sourceKey !== legacy.sourceKey);
    state.messages = mergeMessage(state.messages, {
      ...legacy,
      sourceKey, contentHex: recovered.contentHex, id: recovered.envelope.id,
      conversationId, contactId: contact.id, wireVersion: recovered.envelope.version,
      direction: "outbound", body: recovered.envelope.body, createdAt: recovered.envelope.createdAt,
      observedAt: new Date().toISOString(), status: legacy?.status || "sent-copy-observed", from: recovered.envelope.from,
      inReplyTo: recovered.envelope.inReplyTo,
    });
    added += 1;
  }
  state.recoveredOutgoingContacts.add(contact.id);
  return added;
}

async function pollContact(contact) {
  if (state.blocked.includes(contact.id) || state.grants[contact.id] !== "grant-ready") return 0;
  const entries = await api.listIncoming(contact);
  let added = 0;
  for (const entry of entries) {
    const sourceKey = `${contact.journal}/${contact.identity}/${entry.name.toLowerCase()}`;
    const existing = state.messages.find((message) => message.sourceKey === sourceKey);
    if (existing) continue;
    try {
      const received = await api.readIncoming(contact, entry.name);
      const now = new Date().toISOString();
      const conversationId = received.envelope.version === 2
        ? conversationIdForGroup(ensureIncomingGroup(received.envelope).id)
        : conversationIdForContact(contact.id);
      state.messages = mergeMessage(state.messages, {
        sourceKey, contentHex: received.contentHex, id: received.envelope.id,
        conversationId, contactId: contact.id, wireVersion: received.envelope.version,
        direction: "inbound", body: received.envelope.body, createdAt: received.envelope.createdAt,
        observedAt: now, status: "validated", from: received.envelope.from, inReplyTo: received.envelope.inReplyTo,
      });
      delete state.quarantine[sourceKey];
      added += 1;
      if (state.initializedPeers.has(contact.id)) notifyIncoming(contact, received.envelope.body);
    } catch (error) {
      state.quarantine[sourceKey] = {
        contactId: contact.id,
        id: entry.name,
        error: error instanceof Error ? error.message.slice(0, 300) : String(error).slice(0, 300),
        observedAt: new Date().toISOString(),
      };
    }
  }
  added += await recoverOutgoingContact(contact);
  state.initializedPeers.add(contact.id);
  return added;
}

function notifyIncoming(contact, body) {
  if (document.visibilityState === "visible") toast(`New message from ${contact.handle}`);
  if (document.visibilityState !== "visible" && Notification.permission === "granted") {
    new Notification(`Message from ${contact.handle}`, { body: "Open Messenger to read it.", icon: new URL("./messenger-mark.svg", location.href).href, tag: `contact-${contact.id}` });
  }
}

async function poll() {
  clearTimeout(state.pollTimer);
  if (state.polling || !state.identity) return schedulePoll();
  state.polling = true;
  if (!state.hasPolled) setConnection("busy", "Checking mail");
  const messageCount = state.messages.length;
  const quarantineBefore = JSON.stringify(state.quarantine);
  try {
    const results = await Promise.allSettled(state.contacts.map(pollContact));
    const failures = results.filter((result) => result.status === "rejected");
    const changed = state.messages.length !== messageCount || JSON.stringify(state.quarantine) !== quarantineBefore;
    if (changed) {
      persist();
      renderView();
    }
    setConnection(failures.length ? "error" : "online", failures.length ? `${failures.length} route errors` : "Mailbox current");
  } catch (error) {
    setConnection("error", "Poll failed");
  } finally {
    state.hasPolled = true;
    state.polling = false; schedulePoll();
  }
}

function schedulePoll(delay) {
  clearTimeout(state.pollTimer);
  const interval = delay ?? (document.visibilityState === "visible" ? config.pollActiveMs : config.pollIdleMs);
  state.pollTimer = setTimeout(poll, interval);
}

function showAuth(message, { retry = false } = {}) {
  clearTimeout(state.pollTimer);
  state.identity = undefined;
  state.localOwner = undefined;
  state.localJournal = undefined;
  state.localIdentityIdBase64 = undefined;
  state.localProfile = undefined;
  elements.auth_message.textContent = message;
  elements.login_button.dataset.mode = retry ? "retry" : "login";
  elements.login_button.textContent = retry ? "Retry startup" : "Continue to sign in";
  elements.auth_screen.hidden = false;
  elements.app.inert = true;
  setConnection(retry ? "error" : "offline", retry ? "Startup blocked" : "Sign in required");
}

function hideAuth() {
  elements.auth_screen.hidden = true;
  elements.login_button.dataset.mode = "login";
  elements.login_button.textContent = "Continue to sign in";
  elements.app.inert = false;
}

async function startupStep(label, operation) {
  try { return await operation(); }
  catch (error) { throw new Error(`${label}: ${error instanceof Error ? error.message : String(error)}`); }
}

async function reconcileContacts() {
  state.grants = Object.fromEntries(state.contacts.map((contact) => [contact.id, "pending"]));
  setConnection("busy", "Reconciling contacts");
  renderView();
  try {
    state.grants = await api.reconcileContactAuthorizations(state.contacts);
  } catch (error) {
    state.grants = Object.fromEntries(state.contacts.map((contact) => [contact.id, "error"]));
    setConnection("error", "Grant reconciliation failed");
    renderView();
    throw error;
  }
  setConnection("online", "Connected");
  renderView();
}

async function establishSession(identity) {
  const username = identity?.identity?.traits?.username;
  if (!username) throw new Error("Kratos session has no username");
  const info = await startupStep("Journal discovery failed", () => api.getLocalJournalInfo());
  const journal = info.name;
  api.setLocalIdentity(username, journal);
  hideAuth();
  const registry = await startupStep("Contact registry failed", () => api.loadContactRegistry({ version: 1, contacts: state.seedContacts }));
  state.contactProfileViews = undefined;
  state.profileMigration = await migrateRouteCanonicalRegistry(
    registry,
    api.contactRegistryEvidence(),
    (document) => api.saveContactRegistry(document),
  );
  state.contacts = await startupStep("Contact validation failed", async () => normalizeContacts(state.profileMigration.document));
  state.identity = identity;
  state.localOwner = username;
  state.localJournal = journal;
  state.localIdentityIdBase64 = info.identityIdBase64;
  if (state.profileMigration.ready) {
    const contactProfileConsumer = createMemoryContactProfiles({
      resolveContact: (contactId) => state.contacts.find((contact) => contact.contactId === contactId),
      fetchCurrent: (contact, options) => api.readProfile({
        owner: contact.owner, journal: contact.journal, route: contact.route, signal: options.signal,
      }),
    });
    state.contactProfileViews = createContactProfileViewCache({
      consumer: contactProfileConsumer,
      resolveContact: (contactId) => state.contacts.find((contact) => contact.contactId === contactId),
    });
  }
  restore();
  elements.account_button.textContent = `${username}@${journal}`;
  if (!state.profileMigration.ready) {
    toast(`Obsolete profile binding retirement did not commit; profiles remain unavailable and messaging continues: ${state.profileMigration.error}`, 9000);
  }
  await startupStep("Contact grant reconciliation failed", reconcileContacts);
  await refreshLocalProfile();
  poll();
}

async function loadIdentity({ quiet = false } = {}) {
  try {
    const identity = await api.whoami();
    if (!state.identity || identity?.identity?.traits?.username !== state.localOwner) await establishSession(identity);
    return true;
  } catch (error) {
    if (error instanceof GatewayError && error.status === 401) {
      showAuth("Sign in through the configured Sync Web identity provider. Messenger never receives an API token.");
      return false;
    }
    if (!quiet) showAuth(error instanceof Error ? error.message : String(error), { retry: true });
    return false;
  }
}

async function beginLogin() {
  elements.login_button.disabled = true;
  if (elements.login_button.dataset.mode === "retry") {
    await loadIdentity();
    elements.login_button.disabled = false;
    return;
  }
  state.authPopup = window.open(config.loginUrl, "sync-messenger-login", "popup,width=520,height=760");
  if (!state.authPopup) {
    elements.login_button.disabled = false;
    toast("Allow the sign-in window, then try again");
    return;
  }
  const deadline = Date.now() + 10 * 60 * 1000;
  while (Date.now() < deadline && !state.identity && !state.authPopup.closed) {
    if (await loadIdentity()) break;
    await new Promise((resolve) => setTimeout(resolve, 750));
  }
  if (state.identity && state.authPopup && !state.authPopup.closed) state.authPopup.close();
  elements.login_button.disabled = false;
}

function beginLogout() {
  window.open(config.logoutUrl, "sync-messenger-logout", "popup,width=520,height=640");
  showAuth("Complete sign-out in the identity-provider window, then sign in again when ready.");
}

function showView(view) {
  state.view = view;
  document.body.classList.toggle("conversation-open", view === "profile");
  renderView();
}

function splitSymbols(value) {
  return value.trim().split(/[\s/]+/).filter(Boolean);
}

function openContactDialog() {
  elements.contact_form.reset();
  elements.contact_dialog.showModal();
}

async function addContact(event) {
  event.preventDefault();
  const identity = elements.contact_identity.value.trim();
  const journal = elements.contact_journal.value.trim();
  const candidate = {
    contactId: `${identity}-${journal}`.toLowerCase().replace(/[^a-z0-9-]+/g, "-").replace(/^-+|-+$/g, ""),
    handle: elements.contact_handle.value.trim(), identity, journal,
    owner: elements.contact_owner.value.trim(),
    route: splitSymbols(elements.contact_route.value),
    incomingPrincipal: splitSymbols(elements.contact_principal.value),
    color: "#245b66",
  };
  try {
    const contact = normalizeContacts({ version: 1, contacts: [candidate] })[0];
    if (state.contacts.some((item) => item.id === contact.id || (item.identity === contact.identity && item.journal === contact.journal))) throw new Error("That contact already exists");
    const next = [...state.contacts, contact];
    await api.saveContactRegistry(contactRegistryDocument(next));
    state.contacts = next; persist(); elements.contact_dialog.close();
    await reconcileContacts();
    toast(`${contact.handle} added with an exact incoming grant`);
  } catch (error) {
    toast(error instanceof Error ? error.message : String(error), 6500);
  }
}

async function removeContact(contact) {
  try {
    assertContactRemovable(contact.id, state.groups);
  } catch (error) {
    toast(error instanceof Error ? error.message : String(error), 6500);
    return;
  }
  if (!window.confirm(`Remove ${contact.handle} and its incoming mailbox grant?`)) return;
  const next = state.contacts.filter((item) => item.id !== contact.id);
  try {
    await api.saveContactRegistry(contactRegistryDocument(next));
    state.contacts = next;
    state.contactProfileViews?.remove(contact.id);
    state.selectedId = undefined; persist();
    await reconcileContacts();
    toast(`${contact.handle} removed and its incoming grant revoked`, 7500);
  } catch (error) {
    toast(`Contact registry changed, but grant reconciliation failed: ${error instanceof Error ? error.message : String(error)}`, 7500);
  }
}

function openGroupDialog() {
  elements.group_contact_options.replaceChildren();
  for (const contact of state.contacts) {
    const label = document.createElement("label"); label.className = "contact-option";
    const checkbox = document.createElement("input"); checkbox.type = "checkbox"; checkbox.name = "contact"; checkbox.value = contact.id;
    label.append(checkbox, document.createTextNode(contact.handle)); elements.group_contact_options.append(label);
  }
  elements.group_name.value = "";
  elements.group_dialog.showModal();
}

function createGroup(event) {
  event.preventDefault();
  const name = elements.group_name.value.trim();
  const contactIds = [...elements.group_form.querySelectorAll('input[name="contact"]:checked')].map((input) => input.value);
  if (!name || contactIds.length < 2 || contactIds.length > 15) return toast("Choose a name and 2 to 15 contacts");
  const id = crypto.randomUUID();
  state.groups.push({ id, name, contactIds, createdAt: new Date().toISOString() });
  persist(); elements.group_dialog.close(); openConversation(conversationIdForGroup(id));
}

async function enableNotifications() {
  if (!("Notification" in window)) return toast("This browser does not expose notifications");
  const permission = await Notification.requestPermission();
  elements.notify_button.textContent = permission === "granted" ? "◆" : "♢";
  toast(permission === "granted" ? "Browser notifications enabled while Messenger is running" : "Notification permission was not granted");
}

function applyTheme(theme) {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem(THEME_KEY, theme);
  elements.theme_button.textContent = theme === "dark" ? "◑" : "◐";
}

function bindEvents() {
  document.querySelectorAll(".rail-button").forEach((button) => button.addEventListener("click", () => showView(button.dataset.view)));
  elements.search_input.addEventListener("input", renderConversationList);
  elements.new_group_button.addEventListener("click", openGroupDialog);
  elements.group_form.addEventListener("submit", createGroup);
  elements.contact_form.addEventListener("submit", addContact);
  document.querySelectorAll(".dialog-close").forEach((button) => button.addEventListener("click", () => button.closest("dialog").close()));
  elements.composer.addEventListener("submit", async (event) => {
    event.preventDefault(); const body = elements.message_input.value.trim(); if (!body || state.sending) return;
    state.sending = true; elements.send_button.disabled = true;
    try { await sendCurrent(body); elements.message_input.value = ""; elements.message_input.style.height = "auto"; }
    catch (error) { toast(error instanceof Error ? error.message : String(error), 6500); }
    finally { state.sending = false; elements.send_button.disabled = false; elements.message_input.focus(); }
  });
  elements.cancel_reply.addEventListener("click", clearReply);
  elements.message_input.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && state.replyTarget) { event.preventDefault(); clearReply(); return; }
    if (event.key === "Enter" && !event.shiftKey && !event.ctrlKey) { event.preventDefault(); elements.composer.requestSubmit(); }
  });
  elements.message_input.addEventListener("input", () => {
    elements.message_input.style.height = "auto";
    elements.message_input.style.height = `${Math.min(elements.message_input.scrollHeight, 160)}px`;
  });
  elements.block_button.addEventListener("click", () => {
    const selected = selectedConversation(); if (!selected || selected.type !== "contact") return;
    state.blocked = state.blocked.includes(selected.ref) ? state.blocked.filter((id) => id !== selected.ref) : [...state.blocked, selected.ref];
    persist(); openConversation(selected.id);
  });
  elements.mobile_back.addEventListener("click", () => document.body.classList.remove("conversation-open"));
  elements.notify_button.addEventListener("click", enableNotifications);
  elements.theme_button.addEventListener("click", () => applyTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark"));
  elements.account_button.addEventListener("click", beginLogout);
  elements.login_button.addEventListener("click", beginLogin);
  document.addEventListener("visibilitychange", () => schedulePoll(250));
  window.addEventListener("storage", (event) => {
    if (!state.identity || event.key !== accountStorageKey()) return;
    restore();
    renderView();
    if (state.selectedId) renderMessages();
  });
}

async function main() {
  const contacts = await fetch(new URL("./contacts.json", location.href), { cache: "no-store" }).then((response) => {
    if (!response.ok) throw new Error("Contacts configuration unavailable"); return response.json();
  });
  state.seedContacts = normalizeContacts(contacts);
  state.contacts = [...state.seedContacts];
  applyTheme(localStorage.getItem(THEME_KEY) || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"));
  bindEvents(); renderView(); await loadIdentity();
  state.sessionTimer = setInterval(() => loadIdentity({ quiet: true }), 60000);
  if ("serviceWorker" in navigator) navigator.serviceWorker.register("./sw.js").catch(() => {});
  if ("Notification" in window && Notification.permission === "granted") elements.notify_button.textContent = "◆";
  try {
    const source = new EventSource(`${config.gatewayBase}/events`, { withCredentials: true });
    source.addEventListener("sync-web-change", () => schedulePoll(100));
  } catch { /* Adaptive polling remains authoritative. */ }
}

main().catch((error) => {
  setConnection("error", "Startup failed");
  toast(error instanceof Error ? error.message : String(error), 10000);
});
