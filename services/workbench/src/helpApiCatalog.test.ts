import fs from 'fs';
import path from 'path';
import { ApiEntry, ApiPermission } from './types/workbench';

const source = fs.readFileSync(path.join(process.cwd(), 'public', 'help-api.json'), 'utf8');
const catalog = JSON.parse(source) as Record<string, ApiEntry>;

const expectedPermissions: Record<ApiPermission, string[]> = {
  any: ['size', 'trace', 'trace-batch', 'info'],
  user: [
    'put!', 'put-batch!', 'use!', 'use-batch!', 'copy!',
    'retrieve', 'retrieve-batch', 'run!',
    'pin!', 'pin-batch!', 'unpin!', 'unpin-batch!',
    'authorizations', 'authorize!', 'deauthorize!',
  ],
  admin: [
    'truncate!', 'prune!', 'prune-batch!', 'config', 'bridge!', 'update-config!', '*secret*', 'delete-bridge!',
    '*admins-get*', '*admins-set*', '*window-set*',
  ],
  root: ['*eval*', '*call*', '*step*', '*set-secret*', '*set-step*', '*set-query*'],
};

it('matches the verified current Interface and exposed root operation inventory', () => {
  const expected = Object.values(expectedPermissions).flat().sort();
  expect(Object.keys(catalog).sort()).toEqual(expected);
  for (const [permission, names] of Object.entries(expectedPermissions)) {
    names.forEach((name) => expect(catalog[name].permission).toBe(permission));
  }
  expect(catalog).not.toHaveProperty('route');
  expect(catalog).not.toHaveProperty('synchronize!');
});

it('has unique source keys, valid permission classes, and complete catalog fields', () => {
  const sourceKeys = Array.from(source.matchAll(/^  "([^"]+)": \{$/gm), (match) => match[1]);
  expect(sourceKeys).toHaveLength(Object.keys(catalog).length);
  expect(new Set(sourceKeys).size).toBe(sourceKeys.length);
  const permissions: ApiPermission[] = ['any', 'user', 'admin', 'root'];
  Object.entries(catalog).forEach(([name, entry]) => {
    expect(entry.description).toEqual(expect.any(String));
    expect(entry.template).toEqual(expect.any(String));
    expect(entry.example).toEqual(expect.any(String));
    expect(permissions).toContain(entry.permission);
  });
});

it('uses flat canonical committed and federated paths', () => {
  for (const [name, entry] of Object.entries(catalog)) {
    expect(`${name}: ${entry.example}`).not.toMatch(/\(-?\d+\s+\(\*state\*/);
    expect(`${name}: ${entry.template}`).not.toMatch(/\(-?\d+\s+\(\*state\*/);
  }
  expect(catalog.retrieve.example).toContain('(path (-1 *state* alice key))');
  expect(catalog['pin!'].example).toContain('(path (-1 *state* alice important-data))');
});

it('makes every user/admin example exercise its labeled minimum role', () => {
  expectedPermissions.user.forEach((name) => {
    expect(catalog[name].template).toContain('(identity (*state* <user>))');
    expect(catalog[name].example).toContain('(identity (*state* alice))');
  });
  expectedPermissions.admin.forEach((name) => {
    expect(catalog[name].template).toContain('(identity (*state* <admin>))');
    expect(catalog[name].example).toContain('(identity (*state* admin))');
  });
  expect(catalog['*secret*'].description).toMatch(/journal-wide Interface authentication secret/);
  expect(catalog['*secret*'].description).toMatch(/Interface public signing key/);
  const ownerPaths: Record<string, string> = {
    retrieve: '(path (-1 *state* alice key))',
    'put!': '(path (*state* alice counter))',
    'use!': '(path (*state* alice counter))',
    'copy!': '(source (*state* alice documents source)) (path (*state* alice documents copy))',
    'pin!': '(path (-1 *state* alice important-data))',
    'unpin!': '(path (-1 *state* alice old-data))',
    'retrieve-batch': '(paths ((-1 *state* alice key)',
    'pin-batch!': '(paths ((-1 *state* alice important-data)',
    'unpin-batch!': '(paths ((-1 *state* alice old-data)',
    'put-batch!': '(paths ((*state* alice a) (*state* alice b)))',
    'use-batch!': '(paths ((*state* alice counter) (*state* alice counter)))',
  };
  Object.entries(ownerPaths).forEach(([name, path]) => {
    expect(catalog[name].example).toContain(path);
  });
});

it('documents exact authorization add/delete symmetry and distinct windows', () => {
  const authorize = catalog['authorize!'].example;
  const deauthorize = catalog['deauthorize!'].example;
  expect(authorize).toContain('(key-index (-32 -1))');
  expect(authorize).toContain('(retrieve (0 -1))');
  expect(deauthorize).toBe(authorize.replace('(function authorize!)', '(function deauthorize!)'));
  expect(catalog['authorize!'].description).toMatch(/local\/public rules omit key-index/i);
});


it('presents exact copy and local administrative retention examples', () => {
  expect(catalog['copy!'].example).toContain('(function copy!)');
  expect(catalog['copy!'].example).toContain('(expected #f) (expression? #t)');
  expect(catalog['truncate!'].description).toMatch(/destructive and irreversible/i);
  expect(catalog['truncate!'].description).toMatch(/Self-local/i);
  expect(catalog['truncate!'].permission).toBe('admin');
  expect(catalog['truncate!'].example).toContain('(identity (*state* admin))');
  expect(catalog['prune!'].description).toMatch(/temporary and permanent Ledger retention/i);
  expect(catalog['prune!'].description).toMatch(/preserving Stage/i);
  expect(catalog['prune-batch!'].description).toMatch(/atomically removes the union/i);
  expect(catalog['prune!'].permission).toBe('admin');
  expect(catalog['prune-batch!'].permission).toBe('admin');
});
