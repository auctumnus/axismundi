const initializeSettings = () => {
    const form = document.getElementById("settings-form") as HTMLFormElement;
    if (!form) return;

    const messageDiv = document.getElementById("settings-message") as HTMLDivElement;
    const genderColorInput = document.getElementById("gender") as HTMLInputElement;
    const genderHexInput = document.getElementById("gender_hex") as HTMLInputElement;
    const profilePictureInput = document.getElementById("profile_picture") as HTMLInputElement;
    const profilePicturePreview = document.getElementById("profile-picture-preview") as HTMLImageElement;

    // sync color picker with hex input
    genderColorInput.addEventListener("input", () => {
        const hex = genderColorInput.value.replace("#", "");
        genderHexInput.value = hex;
    });

    genderHexInput.addEventListener("input", () => {
        let hex = genderHexInput.value.replace("#", "");
        if (/^[a-fA-F0-9]{6}$/.test(hex)) {
            genderColorInput.value = `#${hex}`;
        }
    });

    // preview profile picture
    profilePictureInput.addEventListener("change", () => {
        const file = profilePictureInput.files?.[0];
        if (file) {
            const reader = new FileReader();
            reader.onload = (e) => {
                profilePicturePreview.src = e.target?.result as string;
            };
            reader.readAsDataURL(file);
        }
    });

    // handle form submission
    form.addEventListener("submit", async (e) => {
        e.preventDefault();

        const formData = new FormData(form);
        const username = formData.get("username") as string;

        // show loading state
        const submitButton = form.querySelector('button[type="submit"]') as HTMLButtonElement;
        const originalButtonText = submitButton.textContent;
        submitButton.disabled = true;
        submitButton.textContent = "saving...";
        messageDiv.style.display = "none";

        try {
            // handle profile picture upload if a file was selected
            const profilePictureFile = profilePictureInput.files?.[0];
            if (profilePictureFile) {
                await uploadProfilePicture(username, profilePictureFile);
            }

            // update user information
            await updateUserInfo(username, formData);

            showMessage("settings saved successfully! ^_^", "success");

            // reload page after a short delay to show updated data
            setTimeout(() => {
                window.location.reload();
            }, 1500);
        } catch (error: any) {
            showMessage(error.message || "failed to save settings :(", "error");
        } finally {
            submitButton.disabled = false;
            submitButton.textContent = originalButtonText;
        }
    });

    const showMessage = (message: string, type: "success" | "error") => {
        messageDiv.textContent = message;
        messageDiv.style.display = "block";
        messageDiv.style.backgroundColor = type === "success" ? "#d4edda" : "#f8d7da";
        messageDiv.style.color = type === "success" ? "#155724" : "#721c24";
        messageDiv.style.border = type === "success" ? "1px solid #c3e6cb" : "1px solid #f5c6cb";
    };

    const uploadProfilePicture = async (username: string, file: File) => {
        const formData = new FormData();
        formData.append("image", file);

        const response = await fetch(`/api/users/${username}/profile-picture`, {
            method: "PUT",
            body: formData,
        });

        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.message || "failed to upload profile picture");
        }
    };

    const updateUserInfo = async (username: string, formData: FormData) => {
        const updateData: any = {};

        // include fields that have values or have been explicitly cleared
        const displayName = (formData.get("display_name") as string)?.trim();
        if (displayName) updateData.display_name = displayName;

        const description = (formData.get("description") as string)?.trim();
        if (description) updateData.description = description;

        const pronouns = (formData.get("pronouns") as string)?.trim();
        if (pronouns) updateData.pronouns = pronouns;

        const gender = (formData.get("gender_hex") as string)?.trim();
        if (gender && /^[a-fA-F0-9]{6}$/.test(gender)) {
            updateData.gender = gender;
        }

        const newUsername = (formData.get("username") as string)?.trim();
        if (newUsername && newUsername !== username) {
            updateData.username = newUsername;
        }

        const email = (formData.get("email") as string)?.trim();
        if (email) updateData.email = email;

        const currentPassword = formData.get("current_password") as string;
        const newPassword = formData.get("new_password") as string;
        if (currentPassword && newPassword) {
            updateData.current_password = currentPassword;
            updateData.new_password = newPassword;
        }

        // only make request if there's something to update
        if (Object.keys(updateData).length === 0) {
            return;
        }

        const response = await fetch(`/api/users/${username}`, {
            method: "PUT",
            headers: {
                "Content-Type": "application/json",
            },
            body: JSON.stringify(updateData),
        });

        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.message || "failed to update user information");
        }
    };
};

document.addEventListener("DOMContentLoaded", () => {
    initializeSettings();
});
