// Controller outline SVGs live in static/bindings/ and are served at the root,
// so the diagram references them by URL (no Vite import needed).
const SVG_ASSETS: Record<string, string> = {
  indexcontroller_right: "/bindings/indexcontroller_right.svg",
  oculus_touch_right: "/bindings/oculus_touch_right.svg",
  vive_wand: "/bindings/vive_wand.svg",
};

export function getSvgAssetUrl(assetKey: string): string | undefined {
  return SVG_ASSETS[assetKey];
}
