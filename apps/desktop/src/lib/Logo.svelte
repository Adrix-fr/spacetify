<script lang="ts">
  // The "Planète" logo: Spotify-like waves on a planet with a ring.
  let { size = 28 }: { size?: number } = $props();
  const uid = Math.random().toString(36).slice(2, 8);
</script>

<svg width={size} height={size} viewBox="0 0 64 64" fill="none" aria-hidden="true">
  <defs>
    <linearGradient id="g-{uid}" x1="10" y1="4" x2="54" y2="60" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#FFA273" />
      <stop offset="0.5" stop-color="#FF6B81" />
      <stop offset="1" stop-color="#E5487F" />
    </linearGradient>
    <clipPath id="p-{uid}"><circle cx="32" cy="33" r="19" /></clipPath>
    <mask id="m-{uid}" maskUnits="userSpaceOnUse" x="0" y="0" width="64" height="64">
      <rect width="64" height="64" fill="#fff" />
      <g fill="none" stroke="#000" stroke-linecap="round">
        <path d="M20 27.5C27 24.5 37 25 45 29" stroke-width="4" />
        <path d="M21.5 33.5C28 31 36 31.5 42.5 34.5" stroke-width="3.4" />
        <path d="M23.5 39.3C29 37.5 34.5 38 38.5 39.6" stroke-width="2.8" />
        <g transform="rotate(-15 32 33)" clip-path="url(#p-{uid})">
          <path d="M2 33A30 9 0 0 0 62 33" stroke-width="6.5" stroke-linecap="butt" />
        </g>
      </g>
    </mask>
  </defs>
  <g mask="url(#m-{uid})">
    <g transform="rotate(-15 32 33)">
      <ellipse cx="32" cy="33" rx="30" ry="9" stroke="url(#g-{uid})" stroke-width="3.5" />
    </g>
    <circle cx="32" cy="33" r="19" fill="url(#g-{uid})" />
  </g>
  <g transform="rotate(-15 32 33)">
    <path d="M2 33A30 9 0 0 0 62 33" stroke="url(#g-{uid})" stroke-width="3.5" />
  </g>
</svg>
