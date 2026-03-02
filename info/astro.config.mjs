import { defineConfig } from "astro/config";
import mdx from "@astrojs/mdx";
import starlight from "@astrojs/starlight";
import logo from "./src/assets/logo.png";

const isGitHubActions = process.env.GITHUB_ACTIONS === "true";
const repoName = process.env.GITHUB_REPOSITORY?.split("/")[1];
const pagesUrl = process.env.GITHUB_PAGES_URL;
const pagesBasePath = (() => {
  if (!pagesUrl) return "";
  try {
    const pathname = new URL(pagesUrl).pathname.replace(/\/$/, "");
    return pathname || "/";
  } catch {
    return "";
  }
})();

export default defineConfig({
  site: pagesUrl ?? "https://sandialabs.github.io",
  base: isGitHubActions
    ? pagesBasePath || (repoName ? `/${repoName}` : "/")
    : "/",
  integrations: [
    starlight({
      title: "Synchronic Web",
      description: "Documentation for usage, operation, development, and research.",
      favicon: "/images/favicon.ico",
      head: [],
      logo: {
        src: logo,
      },
      customCss: ["./src/styles/custom.css"],
      sidebar: [
        {
          label: "Documentation",
          items: [
            { slug: "index", label: "Overview" },
            { slug: "usage", label: "Usage" },
            { slug: "operation", label: "Operation" },
            { slug: "development", label: "Development" },
            { slug: "research", label: "Research" },
            { slug: "glossary", label: "Glossary" },
          ],
        },
      ],
    }),
    mdx(),
  ],
});
