/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{svelte,js,ts}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // Algorand teal
        algo: {
          50: '#e6fff9',
          100: '#b3ffea',
          200: '#80ffdb',
          300: '#4dffcc',
          400: '#1affbd',
          500: '#00d4aa',
          600: '#00a584',
          700: '#00765f',
          800: '#00483a',
          900: '#001a15',
        },
        surface: {
          dark: '#0d1117',
          DEFAULT: '#161b22',
          light: '#f6f8fa',
        },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Menlo', 'monospace'],
      },
    },
  },
  plugins: [],
};
