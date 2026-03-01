import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const infoDir = path.resolve(__dirname, "..");

const baseUrl = process.env.SYNC_BASE_URL || "http://127.0.0.1:8192";
const outputDir =
  process.env.SYNC_SCREENSHOT_DIR || path.join(infoDir, "docs", "images", "screenshots");
const settleMs = Number(process.env.SYNC_SCREENSHOT_SETTLE_MS || "1500");

const targets = [
  {
    key: "explorer",
    url: `${baseUrl}/explorer/`,
    selector: "body",
  },
  {
    key: "workbench",
    url: `${baseUrl}/workbench/`,
    selector: "body",
  },
];

const variants = [
  {
    key: "desktop",
    contextOptions: {
      viewport: { width: 1536, height: 960 },
      deviceScaleFactor: 1,
    },
  },
];

async function run() {
  let playwright;
  try {
    playwright = await import("playwright");
  } catch (err) {
    console.error("Playwright is not installed.");
    console.error("Run: npm install && npx playwright install chromium");
    throw err;
  }

  await fs.mkdir(outputDir, { recursive: true });
  const { chromium } = playwright;
  const browser = await chromium.launch({ headless: true });
  const created = [];

  try {
    for (const variant of variants) {
      const context = await browser.newContext(variant.contextOptions);
      const page = await context.newPage();

      for (const target of targets) {
        const url = target.url;
        const outPath = path.join(outputDir, `${target.key}-${variant.key}.png`);
        console.log(`Capturing ${url} -> ${outPath}`);

        await page.goto(url, { waitUntil: "domcontentloaded", timeout: 30000 });
        await page.waitForSelector(target.selector, { timeout: 10000 });
        await page.waitForTimeout(settleMs);
        await page.screenshot({ path: outPath, fullPage: false });
        created.push(outPath);
      }

      await context.close();
    }
  } finally {
    await browser.close();
  }

  console.log("");
  console.log("Captured screenshots:");
  for (const file of created) {
    console.log(`- ${file}`);
  }
}

run().catch((err) => {
  console.error("");
  console.error("Screenshot capture failed.");
  console.error("Verify the UI stack is running and reachable at:", baseUrl);
  console.error("Underlying error:", err?.message || err);
  process.exit(1);
});
