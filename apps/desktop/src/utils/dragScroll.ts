/**
 * How far to scroll per frame while a drag hovers at `pointerY` over a scroller spanning
 * `top`..`bottom`: negative near the top edge, positive near the bottom one, growing as the
 * pointer nears the edge, `0` elsewhere.
 */
export function edgeScrollStep(
  pointerY: number,
  top: number,
  bottom: number,
  edgePx = 48,
  maxStepPx = 18,
): number {
  const intoTop = top + edgePx - pointerY;
  if (intoTop > 0) {
    return -Math.ceil((Math.min(intoTop, edgePx) / edgePx) * maxStepPx);
  }
  const intoBottom = pointerY - (bottom - edgePx);
  if (intoBottom > 0) {
    return Math.ceil((Math.min(intoBottom, edgePx) / edgePx) * maxStepPx);
  }
  return 0;
}
