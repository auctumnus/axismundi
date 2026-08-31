import createPanZoom, { type PanZoom } from 'panzoom';

const ENHANCED = 'data-panzoom-enhanced';
const CLICK_DRAG_THRESHOLD_PX = 5;
const VIEW_PADDING_REM = 0.5;
const MIN_INITIAL_FOCUS_ZOOM = 2.5;
const MAX_INITIAL_FOCUS_SCALE = 1;

function findScene(svg: SVGSVGElement): SVGGraphicsElement | null {
  for (const child of Array.from(svg.children)) {
    if (child instanceof SVGGElement) return child;
  }
  return null;
}

function getRootFontSize(): number {
  const fontSize = parseFloat(
    getComputedStyle(document.documentElement).fontSize,
  );
  return Number.isFinite(fontSize) ? fontSize : 16;
}

function getViewPadding(rect: DOMRect): number {
  const padding = VIEW_PADDING_REM * getRootFontSize();
  return Math.min(padding, rect.width / 2, rect.height / 2);
}

function computeFitScale(
  svg: SVGSVGElement,
  scene: SVGGraphicsElement,
): number {
  const bbox = scene.getBBox();
  const rect = svg.getBoundingClientRect();
  const padding = getViewPadding(rect);
  const availableWidth = rect.width - padding * 2;
  const availableHeight = rect.height - padding * 2;
  if (
    bbox.width === 0 ||
    bbox.height === 0 ||
    availableWidth <= 0 ||
    availableHeight <= 0
  ) {
    return 1;
  }
  return Math.min(availableWidth / bbox.width, availableHeight / bbox.height);
}

function fitToContainer(
  svg: SVGSVGElement,
  scene: SVGGraphicsElement,
  instance: PanZoom,
  focusTarget: SVGGraphicsElement | null = null,
) {
  // Cancel an in-flight smooth zoom before restoring the initial framing.
  instance.zoomTo(0, 0, 1);

  const bbox = scene.getBBox();
  const rect = svg.getBoundingClientRect();
  if (
    bbox.width === 0 ||
    bbox.height === 0 ||
    rect.width === 0 ||
    rect.height === 0
  )
    return;

  const padding = getViewPadding(rect);
  const fitScale = computeFitScale(svg, scene);
  const scale = focusTarget
    ? Math.max(
        fitScale,
        Math.min(fitScale * MIN_INITIAL_FOCUS_ZOOM, MAX_INITIAL_FOCUS_SCALE),
      )
    : fitScale;
  const minX = padding - bbox.x * scale;
  const maxX = rect.width - padding - (bbox.x + bbox.width) * scale;
  const minY = padding - bbox.y * scale;
  const maxY = rect.height - padding - (bbox.y + bbox.height) * scale;

  let x = (minX + maxX) / 2;
  let y = (minY + maxY) / 2;
  if (focusTarget) {
    const focusBox = focusTarget.getBBox();
    const desiredX = rect.width / 2 - (focusBox.x + focusBox.width / 2) * scale;
    const desiredY =
      rect.height / 2 - (focusBox.y + focusBox.height / 2) * scale;
    if (scale > fitScale) {
      x = desiredX;
      y = desiredY;
    } else {
      x = Math.max(minX, Math.min(desiredX, maxX));
      y = Math.max(minY, Math.min(desiredY, maxY));
    }
  }

  // Panzoom reads Graphviz's inherited SVG transform as its initial scale and
  // can reject programmatic zooms while settling bounds. Set its exposed model
  // directly, then let moveTo apply the transform and enforce the bounds.
  instance.getTransform().scale = scale;
  instance.moveTo(x, y);
}

function makeButton(label: string, iconName: string): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.setAttribute('aria-label', label);
  btn.title = label;
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('class', 'icon');
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS('http://www.w3.org/2000/svg', 'use');
  use.setAttribute('href', `#icon-${iconName}`);
  svg.appendChild(use);
  btn.appendChild(svg);
  return btn;
}

function makeControls(
  svg: SVGSVGElement,
  scene: SVGGraphicsElement,
  instance: PanZoom,
  focusTarget: SVGGraphicsElement | null,
): HTMLDivElement {
  const wrap = document.createElement('div');
  wrap.className = 'panzoom-controls';

  const zoomAt = (factor: number) => {
    const rect = svg.getBoundingClientRect();
    instance.smoothZoom(
      rect.left + rect.width / 2,
      rect.top + rect.height / 2,
      factor,
    );
  };

  const zoomIn = makeButton('zoom in', 'plus');
  zoomIn.addEventListener('click', () => zoomAt(1.4));

  const zoomOut = makeButton('zoom out', 'minus');
  zoomOut.addEventListener('click', () => zoomAt(1 / 1.4));

  const reset = makeButton('reset view', 'refresh');
  reset.addEventListener('click', () => {
    fitToContainer(svg, scene, instance, focusTarget);
  });

  wrap.append(zoomIn, zoomOut, reset);
  // stop pointer events from initiating a pan on the svg
  wrap.addEventListener('pointerdown', (e) => e.stopPropagation());
  return wrap;
}

function enhance(container: HTMLElement): PanZoom | null {
  if (container.hasAttribute(ENHANCED)) return null;

  const svg = container.querySelector('svg');
  if (!svg) return null;
  const scene = findScene(svg);
  if (!scene) return null;

  const naturalRect = svg.getBoundingClientRect();
  const targetHeight = Math.max(
    240,
    Math.min(naturalRect.height || 480, window.innerHeight * 0.7),
  );

  container.setAttribute(ENHANCED, '');
  container.style.overflow = 'hidden';
  container.style.touchAction = 'none';
  container.style.position = 'relative';
  container.style.height = `${targetHeight}px`;
  container.style.justifyContent = 'flex-start';
  svg.style.width = '100%';
  svg.style.height = '100%';
  svg.style.cursor = 'grab';
  svg.removeAttribute('width');
  svg.removeAttribute('height');

  let downX = 0;
  let downY = 0;
  svg.addEventListener('pointerdown', (e) => {
    downX = e.clientX;
    downY = e.clientY;
  });
  svg.addEventListener(
    'click',
    (e) => {
      const dx = e.clientX - downX;
      const dy = e.clientY - downY;
      if (Math.hypot(dx, dy) > CLICK_DRAG_THRESHOLD_PX) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    true,
  );

  const fitScale = computeFitScale(svg, scene);
  const focusSelector = container.dataset.panzoomFocus;
  const focusTarget = focusSelector
    ? svg.querySelector<SVGGraphicsElement>(focusSelector)
    : null;
  const instance = createPanZoom(scene, {
    smoothScroll: false,
    bounds: true,
    boundsPadding: 0.1,
    minZoom: fitScale * 0.5,
    maxZoom: fitScale * 8,
    zoomDoubleClickSpeed: 1,
    beforeWheel: (e) => !e.ctrlKey && !e.metaKey,
  });

  fitToContainer(svg, scene, instance, focusTarget);

  svg.addEventListener('keydown', (e) => {
    if (e.key === '0') {
      e.preventDefault();
      fitToContainer(svg, scene, instance, focusTarget);
    }
  });

  container.appendChild(makeControls(svg, scene, instance, focusTarget));

  return instance;
}

function enhanceAll() {
  const containers = document.querySelectorAll<HTMLElement>('[data-panzoom]');
  containers.forEach((c) => {
    try {
      enhance(c);
    } catch (err) {
      console.warn('panzoom enhance failed', err);
    }
  });
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', enhanceAll);
} else {
  enhanceAll();
}
