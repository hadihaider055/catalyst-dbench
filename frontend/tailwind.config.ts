import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // VS Code-inspired dark palette
        surface: {
          DEFAULT: "#1e1e1e",
          raised: "#252526",
          overlay: "#2d2d30",
          border: "#3e3e42",
        },
        accent: {
          DEFAULT: "#0078d4",
          hover: "#106ebe",
          muted: "#1a3a5c",
        },
        text: {
          primary: "#cccccc",
          secondary: "#969696",
          muted: "#6a6a6a",
          link: "#3794ff",
        },
        db: {
          postgres: "#336791",
          mysql: "#f29111",
          sqlite: "#003b57",
          mongodb: "#47a248",
          redis: "#dc382d",
        },
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "Menlo", "Consolas", "monospace"],
      },
      fontSize: {
        "2xs": ["10px", "14px"],
        xs: ["11px", "16px"],
        sm: ["12px", "18px"],
        base: ["13px", "20px"],
      },
    },
  },
  plugins: [],
} satisfies Config;
