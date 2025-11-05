document.addEventListener("DOMContentLoaded", () => {
    const body = document.body;

    const openSidebarBtn = document.getElementById("open-sidebar")!;
    const safetyTriangle = document.getElementById("safety-triangle")!;
    const sidebar = document.getElementById("sidebar-panel")!;

    let triangleActive = false;

    const isSidebar = (el: EventTarget | null): boolean => {
        return el === sidebar || el === openSidebarBtn || el === safetyTriangle || sidebar.contains(el as Node);
    }

    const openSidebar = () => {
        body.classList.add("sidebar-open");

        triangleActive = true;
    }

    const closeSidebar = () => {
        body.classList.remove("sidebar-open");

        triangleActive = false;
        safetyTriangle.classList.remove("active");
    }

    openSidebarBtn.addEventListener("mouseover", () => {
        openSidebar();
    });

    openSidebarBtn.addEventListener("mouseleave", (event) => {
        if(!isSidebar(event.relatedTarget)) {
            closeSidebar();
        }
    });

    safetyTriangle.addEventListener("mouseleave", (event) => {
        if(triangleActive && !isSidebar(event.relatedTarget)) {
            closeSidebar();
        }
    });

    sidebar.addEventListener("mouseleave", (event) => {
        if(!isSidebar(event.relatedTarget)) {
            closeSidebar();
        }
    });
});