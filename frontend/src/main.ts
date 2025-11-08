const initializeUserPanel = () => {
    const currentUserElement = document.getElementById("current-user")!;
    if(!currentUserElement) {
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
    }

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
    }

    const unlockUserPanel = () => {
        userPanel.classList.remove("active");
        safetyTriangle.classList.remove("active");
        panelLocked = false;
    }

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
        console.log(e.relatedTarget);
        if(!inUserPanel(e.relatedTarget as HTMLElement) && !panelLocked && e.relatedTarget !== safetyTriangle) {
            closeUserPanel();
        }
    });

    safetyTriangle.addEventListener("mouseleave", (e) => {
        if(triangleActive && !inUserPanel(e.relatedTarget as HTMLElement) && !panelLocked && e.relatedTarget !== currentUserElement) {
            closeUserPanel();
        }
    });

    userPanel.addEventListener("mouseleave", (e) => {
        if(!panelLocked && e.relatedTarget !== currentUserElement && e.relatedTarget !== safetyTriangle) {
            closeUserPanel();
        }
    });
}


document.addEventListener("DOMContentLoaded", () => {
    initializeUserPanel();
});