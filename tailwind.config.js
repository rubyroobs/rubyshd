const colors = require('tailwindcss/colors');

/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./public_root/**/*.{html,html.hbs,js}", "./errdocs/**/*.{html,html.hbs,js}", "./partials/**/*.{html,html.hbs,js}"],
  theme: {
    extend: {},
    colors: {
      background: 'var(--color-background)',
      border: 'var(--color-border)',
      text: 'var(--color-text)',
      link: 'var(--color-link)',
      hover: 'var(--color-hover)',
      accent: 'var(--color-accent)',
    },
    fontFamily: {
      'header': 'var(--font-family-header)',
      'sans': 'var(--font-family-sans)',
      'serif': 'var(--font-family-serif)',
      'mono': 'var(--font-family-mono)',
    }
  },
  plugins: [],
}