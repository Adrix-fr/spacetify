<script lang="ts">
  // A tiny drawing of Spotify's layout in a theme's colours.
  import type { Palette } from "./api";
  let { palette, height = 110 }: { palette: Palette; height?: number } = $props();
</script>

<div class="preview" style:background={palette.frame} style:height="{height}px" aria-hidden="true">
  <div class="side" style:background={palette.background}>
    <span style:background={palette.highlight}></span>
    <span style:background={palette.highlight}></span>
  </div>
  <div class="main" style:background={palette.background}>
    <span class="line" style:background={palette.elevated} style:width="62%"></span>
    <span class="line" style:background={palette.text} style:width="38%" style:opacity="0.55"></span>
    <div class="tiles">
      <span style:background={palette.highlight}></span>
      <span style:background={palette.highlight}></span>
      <span style:background={palette.highlight}></span>
    </div>
  </div>
  <div class="bar">
    <span class="play" style:background={palette.accent}></span>
  </div>
</div>

<style>
  .preview {
    display: grid;
    grid-template-columns: 46px 1fr;
    grid-template-rows: 1fr 18px;
    gap: 6px;
    padding: 8px;
  }
  .side,
  .main {
    border-radius: 6px;
    padding: 9px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow: hidden;
  }
  .side span {
    height: 8px;
    border-radius: 4px;
  }
  .line {
    height: 8px;
    border-radius: 4px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 5px;
    margin-top: 2px;
  }
  .tiles span {
    height: 22px;
    border-radius: 4px;
  }
  .bar {
    grid-column: 1 / 3;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .play {
    width: 13px;
    height: 13px;
    border-radius: 50%;
  }
</style>
