export const allowHttpSwaggerAssets = (header: string): string =>
  header.replace(/\s*upgrade-insecure-requests;?/g, "").trim();
