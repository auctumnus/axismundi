import { initializeLikeButtons } from "./components/like-button";
import { initializeTimeElements } from "./components/time";
import { initializeTooltips } from "./components/tooltip/tooltip.ts";
import { initializeUserPanel } from "./layout/header";
import { initializeMobileNavSidebar } from "./layout/sidebar";

document.addEventListener("DOMContentLoaded", () => {
  initializeUserPanel();
  initializeMobileNavSidebar();
  initializeTooltips();
  initializeLikeButtons();
  initializeTimeElements();
});
