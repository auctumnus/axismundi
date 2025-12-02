import { autoPlacement, autoUpdate, computePosition, offset } from "@floating-ui/dom";


const setupTooltip = (target: HTMLElement) => {
    const tooltipText = target.getAttribute("data-tooltip");
    if (!tooltipText) return;

    const tooltip = document.createElement("span");
    tooltip.className = "tooltip";
    tooltip.textContent = tooltipText;
    tooltip.setAttribute("role", "tooltip");
    target.appendChild(tooltip);

    let cleanup: (() => void) | undefined;

    const updatePosition = () => {
        computePosition(target, tooltip, {
            middleware: [
                offset(4),
                autoPlacement()
            ],
        }).then(({ x, y }) => {
            Object.assign(tooltip.style, {
                left: `${x}px`,
                top: `${y}px`,
            });
        });
    };

    const showTooltip = () => {
        tooltip.classList.add("visible");
        updatePosition();
        cleanup = autoUpdate(target, tooltip, updatePosition);
    };

    const hideTooltip = () => {
        tooltip.classList.remove("visible");
        if (cleanup) {
            cleanup();
            cleanup = undefined;
        }
    };

    ([
        ["mouseenter", showTooltip],
        ["mouseleave", hideTooltip],
        ["focus", showTooltip],
        ["blur", hideTooltip],
    ] as const).forEach(([event, handler]) => {
        target.addEventListener(event, handler);
    });
};

document.addEventListener("DOMContentLoaded", () => {
    const tooltipTargets = document.querySelectorAll('[data-tooltip]');
    for (const target of tooltipTargets) {
        setupTooltip(target as HTMLElement);
    }
});