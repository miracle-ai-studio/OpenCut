export const previewZoomPresets = [25, 50, 75, 100, 150, 200];
export const clampPreviewZoom = (zoom: number) =>
  Math.min(16, Math.max(0.25, zoom));
export function previewGeometry(
  width: number,
  height: number,
  viewportWidth: number,
  viewportHeight: number,
  zoom: number,
  pan: { x: number; y: number },
) {
  const fit =
    width > 0 && height > 0 && viewportWidth > 48 && viewportHeight > 48
      ? Math.min((viewportWidth - 48) / width, (viewportHeight - 48) / height)
      : 1;
  const scale = fit * clampPreviewZoom(zoom);
  const overflow = {
    x: Math.max(0, (width * scale - (viewportWidth - 48)) / 2),
    y: Math.max(0, (height * scale - (viewportHeight - 48)) / 2),
  };
  return {
    fit,
    scale,
    width: width * scale,
    height: height * scale,
    pan: {
      x:
        overflow.x < 1e-6
          ? 0
          : Math.min(overflow.x, Math.max(-overflow.x, pan.x)),
      y:
        overflow.y < 1e-6
          ? 0
          : Math.min(overflow.y, Math.max(-overflow.y, pan.y)),
    },
    overflow,
  };
}
