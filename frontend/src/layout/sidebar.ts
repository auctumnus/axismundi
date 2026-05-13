export const initializeMobileNavSidebar = () => {
  const logomark = document.getElementById("logomark");
  const sidebar = document.querySelector(
    "footer ul.sections",
  ) as HTMLElement | null;
  if (!logomark || !sidebar) return;

  const isMobile = () => window.innerWidth < 1200;

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
    if (!isMobile() && document.body.classList.contains("nav-sidebar-open")) {
      closeSidebar();
    }
  });
};
