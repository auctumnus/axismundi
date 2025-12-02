import { autoUpdate, computePosition, flip, offset } from "@floating-ui/dom";

document.addEventListener("DOMContentLoaded", () => {
    const badgeTargets = document.querySelectorAll('[data-badge]');
    for (const target of badgeTargets) {
        const username = target.getAttribute("data-badge");
        if (!username) continue;

        const badge = document.createElement("div");
        badge.className = "hover-badge";
        badge.textContent = `Loading info for ${username}...`;
        target.appendChild(badge);
        
        let cleanup: (() => void) | undefined;
    }
})