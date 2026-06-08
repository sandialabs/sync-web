# Locust Load Testing for Synchronic Web Ledger

This directory contains a Locust load testing script for testing the synchronic web ledger service.

## Prerequisites

1. Start the current general compose stack from `sync-web`:
   - `COMPOSE_PROJECT_NAME=sync-local SECRET=password HTTP_PORT=8192 tests/api/local-compose.sh up`
   - or `COMPOSE_PROJECT_NAME=sync-dev SECRET=password HTTP_PORT=8192 HTTPS_PORT=8193 docker compose -f deploy/compose/general/compose.yaml up -d`

2. Create an API token from `/auth/settings` or `POST /api/v1/tokens`.
3. Set `API_TOKEN` to the plaintext token returned at creation time.

## Running Tests

### Interactive Web UI Mode (Recommended for Development)

```bash
$ API_TOKEN=sync-... locust --host=http://localhost:8192
```

**Expected Behavior:**
1. Locust starts and displays: `Starting web interface at http://localhost:8089`
2. Open your browser to http://localhost:8089
3. You'll see the Locust web interface with:
   - **Host**: Pre-filled with `http://localhost:8192`
   - **Number of users**: Input field for concurrent users
   - **Spawn rate**: Input field for users spawned per second
4. Enter desired values (e.g., 10 users, 2 spawn rate) and click **"Start swarming"**
5. Monitor real-time statistics including:
   - Requests per second
   - Response times
   - Success/failure rates
   - Individual request logs in the terminal

### Headless Mode (For Automated Testing)

```bash
$ API_TOKEN=sync-... locust --host=http://localhost:8192 --users=10 --spawn-rate=2 --run-time=60s --headless
```

**Expected Behavior:**
1. Test starts immediately without web interface
2. Runs for specified duration (60 seconds in example)
3. Outputs statistics to terminal
4. Exits automatically when complete

### Additional Options

```bash
# Save results to CSV files
$ API_TOKEN=sync-... locust --host=http://localhost:8192 --users=10 --spawn-rate=2 --run-time=60s --headless --csv=results

# Run with custom web UI port
$ API_TOKEN=sync-... locust --host=http://localhost:8192 --web-port=8090
```

## Test Behavior

The load test performs the following actions:
- Generates random key-value pairs
- Sends authenticated POST requests to `/api/v1/general/set` through the gateway
- Each request sets `(*state* locust <key>)` to a random string value via `set!`
- Logs both request and response (truncated to 80 characters each)

## Expected Output

In the terminal, you'll see output like:
```
REQ: {"path":["*state*","locust","key-123456"],"value":"val-789012","expression?":true} | RESP: true
REQ: {"path":["*state*","locust","key-234567"],"value":"val-890123","expression?":true} | RESP: true
```

## Troubleshooting

- **Connection errors**: Ensure the ledger server is running and accessible
- **Authentication errors**: Verify `API_TOKEN` is set to a valid token for the target gateway user
- **Web UI not accessible**: Check that port 8089 (or custom port) is not in use
