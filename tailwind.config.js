/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        // Soft-amber (paper/surfaces)
        amber: {
          50: '#f9f7f3',
          100: '#f1ebe3',
          200: '#d8c9b5',
        },
        // Japonica (accent)
        jap: {
          50: '#fcf4f0',
          100: '#f8e6dc',
          200: '#f0cbb8',
          300: '#e6a78b',
          400: '#d97757',
          600: '#c44632',
          700: '#a3352b',
        },
        // Armadillo (text)
        arm: {
          200: '#e8e5e3',
          300: '#d8d3cf',
          400: '#ada299',
          500: '#7e7266',
          600: '#5b554a',
          700: '#4a433a',
          800: '#2b2422',
          900: '#1e1915',
          950: '#0d0a08',
        },
      },
      fontFamily: {
        serif: ['Fraunces', 'Georgia', 'serif'],
        sans: ['Inter Tight', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'ui-monospace', 'monospace'],
      },
      borderRadius: {
        sm: '8px',
        md: '12px',
        lg: '16px',
      },
      fontSize: {
        xs: '11px',
        sm: '13.5px',
      },
    },
  },
  plugins: [],
}
