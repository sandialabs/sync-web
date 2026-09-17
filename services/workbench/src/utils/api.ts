const journalErrorFromValue = (value: unknown): string | undefined => {
  if (!Array.isArray(value) || value[0] !== 'error') return undefined;
  const quoted = value[1];
  const tag = quoted && typeof quoted === 'object' && !Array.isArray(quoted)
    ? (quoted as Record<string, unknown>)['*type/quoted*']
    : undefined;
  return typeof tag === 'string' ? `Journal error: ${tag}` : 'Journal error';
};

const schemeCodecEndpoint = (endpoint: string): string => {
  const url = new URL(endpoint, window.location.href);
  const segments = url.pathname.split('/').filter(Boolean);
  if (segments[segments.length - 1] !== 'interface') {
    throw new Error('Journal endpoint must end with the interface path segment');
  }
  segments.push('scheme-to-json');
  url.pathname = `/${segments.join('/')}`;
  url.search = '';
  url.hash = '';
  return url.toString();
};

const canonicalJournalError = async (
  endpoint: string,
  responseText: string,
  parsedJson: unknown,
  wasJson: boolean,
): Promise<string | undefined> => {
  if (wasJson) return journalErrorFromValue(parsedJson);
  try {
    const response = await fetch(schemeCodecEndpoint(endpoint), {
      method: 'POST',
      headers: { 'Content-Type': 'application/scheme' },
      body: responseText,
    });
    if (!response.ok) return 'Journal response could not be parsed';
    return journalErrorFromValue(JSON.parse(await response.text()));
  } catch {
    return 'Journal response could not be parsed';
  }
};

/**
 * Execute a query against the journal endpoint
 */
export const executeQuery = async (
  endpoint: string,
  query: string
): Promise<{ result: any; request: string; response: string; error?: string }> => {
  const requestBody = query.trim();
  const requestInfo = `POST ${endpoint}\nContent-Type: text/plain\n\n${requestBody}`;

  try {
    const response = await fetch(endpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'text/plain',
      },
      body: requestBody,
    });

    const responseText = await response.text();
    const responseInfo = `HTTP ${response.status} ${response.statusText}\n\n${responseText}`;

    if (!response.ok) {
      return {
        result: null,
        request: requestInfo,
        response: responseInfo,
        error: `Request failed: ${response.status} ${response.statusText}`,
      };
    }

    let result: any;
    let wasJson = true;
    try {
      result = JSON.parse(responseText);
    } catch {
      result = responseText;
      wasJson = false;
    }

    return {
      result,
      request: requestInfo,
      response: responseInfo,
      error: await canonicalJournalError(endpoint, responseText, result, wasJson),
    };
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : 'Unknown error';
    return {
      result: null,
      request: requestInfo,
      response: `Error: ${errorMessage}`,
      error: errorMessage,
    };
  }
};
