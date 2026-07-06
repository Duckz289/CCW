/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: ["ui-sans-serif", "SF Pro Display", "Segoe UI Variable", "Segoe UI", "system-ui", "sans-serif"],
        mono: ["SFMono-Regular", "Cascadia Code", "Consolas", "monospace"],
      },
      colors: {
        ink: "#f3ead7",
        panel: "#fff8e8",
        panel2: "#dfe8c3",
        line: "#d4c59d",
        text: "#1f2b18",
        muted: "#69745c",
        accent: "#2f6b3f",
        warn: "#a86619",
        danger: "#ad3f2f",
      },
      boxShadow: {
        soft: "0 18px 60px rgb(79 65 34 / 0.12)",
      },
    },
  },
  plugins: [],
};
