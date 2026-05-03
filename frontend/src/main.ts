const initializeUserPanel = () => {
  const currentUserElement = document.getElementById("current-user")!;
  if (!currentUserElement) {
    return;
  }

  const userPanel = document.querySelector("nav.user-panel")! as HTMLElement;
  const safetyTriangle = document.getElementById("safety-triangle")!;

  let panelLocked = false;
  let triangleActive = false;

  const inUserPanel = (element: HTMLElement | null): boolean => {
    if (!element) return false;
    if (element === userPanel) return true;
    return inUserPanel(element.parentElement);
  };

  const openUserPanel = () => {
    userPanel.classList.add("active");
    safetyTriangle.classList.add("active");
    triangleActive = true;
  };

  const closeUserPanel = () => {
    userPanel.classList.remove("active");
    safetyTriangle.classList.remove("active");
    triangleActive = false;
  };

  const lockUserPanel = () => {
    userPanel.classList.add("active");
    safetyTriangle.classList.remove("active");
    panelLocked = true;
  };

  const unlockUserPanel = () => {
    userPanel.classList.remove("active");
    safetyTriangle.classList.remove("active");
    panelLocked = false;
  };

  currentUserElement.addEventListener("click", (e) => {
    e.stopPropagation();
    if (panelLocked) {
      unlockUserPanel();
    } else {
      lockUserPanel();
    }
  });

  document.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    if (inUserPanel(target) || target === currentUserElement) {
      return;
    }

    if (panelLocked) {
      unlockUserPanel();
    } else if (triangleActive) {
      closeUserPanel();
    }
  });

  currentUserElement.addEventListener("mouseover", () => {
    if (!panelLocked) {
      openUserPanel();
    }
  });

  currentUserElement.addEventListener("mouseleave", (e) => {
    if (
      !inUserPanel(e.relatedTarget as HTMLElement) &&
      !panelLocked &&
      e.relatedTarget !== safetyTriangle
    ) {
      closeUserPanel();
    }
  });

  safetyTriangle.addEventListener("mouseleave", (e) => {
    if (
      triangleActive &&
      !inUserPanel(e.relatedTarget as HTMLElement) &&
      !panelLocked &&
      e.relatedTarget !== currentUserElement
    ) {
      closeUserPanel();
    }
  });

  userPanel.addEventListener("mouseleave", (e) => {
    if (
      !panelLocked &&
      e.relatedTarget !== currentUserElement &&
      e.relatedTarget !== safetyTriangle
    ) {
      closeUserPanel();
    }
  });
};

const initializeMobileNavSidebar = () => {
  const logomark = document.getElementById("logomark");
  const sidebar = document.querySelector(
    "footer ul.sections",
  ) as HTMLElement | null;
  if (!logomark || !sidebar) return;

  const isMobile = () => window.innerWidth < 1200;

  document.body.classList.add("js-mobile-nav");

  const backdrop = document.createElement("div");
  backdrop.id = "nav-sidebar-backdrop";
  document.body.appendChild(backdrop);

  const openSidebar = () => {
    document.body.classList.add("nav-sidebar-open");
    document.body.classList.remove("nav-sidebar-closing");
    document.body.classList.add("nav-sidebar-opening");
    setTimeout(() => {
      document.body.classList.remove("nav-sidebar-opening");
    }, 300);
    backdrop.classList.add("active");
  };

  const closeSidebar = () => {
    document.body.classList.remove("nav-sidebar-open");
    document.body.classList.add("nav-sidebar-closing");
    document.body.classList.remove("nav-sidebar-opening");
    setTimeout(() => {
      document.body.classList.remove("nav-sidebar-closing");
    }, 300);
    backdrop.classList.remove("active");
  };

  logomark.addEventListener("click", (e) => {
    if (!isMobile()) return;
    e.preventDefault();
    if (document.body.classList.contains("nav-sidebar-open")) {
      closeSidebar();
    } else {
      openSidebar();
    }
  });

  backdrop.addEventListener("click", closeSidebar);

  document.addEventListener("keydown", (e) => {
    if (
      e.key === "Escape" &&
      document.body.classList.contains("nav-sidebar-open")
    ) {
      closeSidebar();
    }
  });

  window.addEventListener("resize", () => {
    if (
      !isMobile() &&
      document.body.classList.contains("nav-sidebar-open")
    ) {
      closeSidebar();
    }
  });
};

document.addEventListener("DOMContentLoaded", () => {
  initializeUserPanel();
  initializeMobileNavSidebar();
});
