import { defineConfig } from "astro/config";
import mdx from "@astrojs/mdx";
import starlight from "@astrojs/starlight";
import logo from "./src/assets/logo.png";

export default defineConfig({
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
