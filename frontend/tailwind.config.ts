import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      keyframes: {
        shrink: {
          "0%": { width: "100%" },
          "100%": { width: "0%" },
        },
        "slide-in-from-right-4": {
          "0%": { transform: "translateX(1rem)", opacity: "0" },
          "100%": { transform: "translateX(0)", opacity: "1" },
        },
        "fade-in": {
          "0%": { opacity: "0" },
          "100%": { opacity: "1" },
        },
      },
      animation: {
        shrink: "shrink 3.5s linear forwards",
        "slide-in-from-right-4": "slide-in-from-right-4 0.2s ease-out",
        "fade-in": "fade-in 0.2s ease-out",
      },
      colors: {
        // Colors reference CSS custom properties so they switch with data-theme
        surface: {
          DEFAULT: "rgb(var(--c-surface) / <alpha-value>)",
          raised:   "rgb(var(--c-surface-raised) / <alpha-value>)",
          overlay:  "rgb(var(--c-surface-overlay) / <alpha-value>)",
          border:   "rgb(var(--c-surface-border) / <alpha-value>)",
        },
        accent: {
          DEFAULT: "rgb(var(--c-accent) / <alpha-value>)",
          hover:   "rgb(var(--c-accent-hover) / <alpha-value>)",
          muted:   "rgb(var(--c-accent-muted) / <alpha-value>)",
        },
        text: {
          primary:   "rgb(var(--c-text-primary) / <alpha-value>)",
          secondary: "rgb(var(--c-text-secondary) / <alpha-value>)",
          muted:     "rgb(var(--c-text-muted) / <alpha-value>)",
          link:      "rgb(var(--c-text-link) / <alpha-value>)",
        },
        db: {
          postgres: "#336791",
          mysql:    "#f29111",
          sqlite:   "#003b57",
          mongodb:  "#47a248",
          redis:    "#dc382d",
        },
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "Menlo", "Consolas", "monospace"],
      },
      fontSize: {
        "2xs": ["10px", "14px"],
        xs:    ["11px", "16px"],
        sm:    ["12px", "18px"],
        base:  ["13px", "20px"],
      },
    },
  },
  plugins: [],
} satisfies Config;
