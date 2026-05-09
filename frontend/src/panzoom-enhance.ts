import createPanZoom, { type PanZoom } from 'panzoom';

const ENHANCED = 'data-panzoom-enhanced';
const CLICK_DRAG_THRESHOLD_PX = 5;

function findScene(svg: SVGSVGElement): SVGGraphicsElement | null {
  for (const child of Array.from(svg.children)) {
    if (child instanceof SVGGElement) return child;
  }
  return null;
}

function fitToContainer(
  svg: SVGSVGElement,
  scene: SVGGraphicsElement,
  instance: PanZoom,
) {
  const bbox = scene.getBBox();
  const rect = svg.getBoundingClientRect();
  if (bbox.width === 0 || bbox.height === 0 || rect.width === 0) return;
  const scale = Math.min(rect.width / bbox.width, rect.height / bbox.height, 1);
  const x = (rect.width - bbox.width * scale) / 2 - bbox.x * scale;
  const y = (rect.height - bbox.height * scale) / 2 - bbox.y * scale;
  instance.zoomAbs(0, 0, scale);
  instance.moveTo(x, y);
}

function focusOn(
  svg: SVGSVGElement,
  target: SVGGraphicsElement,
  instance: PanZoom,
) {
  const bbox = target.getBBox();
  const rect = svg.getBoundingClientRect();
  if (bbox.width === 0 || bbox.height === 0 || rect.width === 0) return;
  // size the target to roughly a quarter of the container
  const scale = Math.min(
    rect.width / (bbox.width * 5),
    rect.height / (bbox.height * 5),
    2,
  );
  const cx = bbox.x + bbox.width / 2;
  const cy = bbox.y + bbox.height / 2;
  const x = rect.width / 2 - cx * scale;
  const y = rect.height / 2 - cy * scale;
  instance.zoomAbs(0, 0, scale);
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
): HTMLDivElement {
  const wrap = document.createElement('div');
  wrap.className = 'panzoom-controls';

  const zoomAt = (factor: number) => {
    const rect = svg.getBoundingClientRect();
    instance.smoothZoom(rect.left + rect.width / 2, rect.top + rect.height / 2, factor);
  };

  const zoomIn = makeButton('zoom in', 'plus');
  zoomIn.addEventListener('click', () => zoomAt(1.4));

  const zoomOut = makeButton('zoom out', 'minus');
  zoomOut.addEventListener('click', () => zoomAt(1 / 1.4));

  const reset = makeButton('reset view', 'refresh');
  reset.addEventListener('click', () => fitToContainer(svg, scene, instance));

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

  const instance = createPanZoom(scene, {
    smoothScroll: false,
    bounds: true,
    boundsPadding: 0.1,
    minZoom: 0.2,
    maxZoom: 8,
    zoomDoubleClickSpeed: 1,
    beforeWheel: (e) => !e.ctrlKey && !e.metaKey,
  });

  const focusSelector = container.dataset.panzoomFocus;
  const focusTarget = focusSelector
    ? svg.querySelector<SVGGraphicsElement>(focusSelector)
    : null;
  if (focusTarget) {
    focusOn(svg, focusTarget, instance);
  } else {
    fitToContainer(svg, scene, instance);
  }

  svg.addEventListener('keydown', (e) => {
    if (e.key === '0') {
      e.preventDefault();
      fitToContainer(svg, scene, instance);
    }
  });

  container.appendChild(makeControls(svg, scene, instance));

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
