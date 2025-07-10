document.addEventListener("DOMContentLoaded", function () {
  // Get the base URL from the mdBook configuration
  const baseUrl = document.location.origin + "/zksync-era/core";

  // Function to create version selector
  function createVersionSelector(versions) {
    const versionSelector = document.createElement("select");
    versionSelector.id = "version-selector";

    // Get the current path
    const currentPath = window.location.pathname;

    // Iterate over the versions object
    for (const [versionName, versionUrl] of Object.entries(versions)) {
      const option = document.createElement("option");
      option.value = versionUrl + "/";
      option.textContent = versionName;

      // Check if the current URL matches this option's value
      if (currentPath.includes(option.value)) {
        option.selected = true; // Set this option as selected
      }

      versionSelector.appendChild(option);
    }

    // Event listener to handle version change
    versionSelector.addEventListener("change", function () {
      const selectedVersion = versionSelector.value;
      // Redirect to the selected version URL
      window.location.href = "/zksync-era/core" + selectedVersion;
    });

    return versionSelector;
  }

  // Fetch versions from JSON file
  fetch(baseUrl + "/versions.json")
    .then((response) => {
      if (!response.ok) {
        throw new Error("Network response was not ok " + response.statusText);
      }
      return response.json();
    })
    .then((data) => {
      const versionSelector = createVersionSelector(data);
      const nav = document.querySelector(".right-buttons");

      if (nav) {
        const versionBox = document.createElement("div");
        versionBox.id = "version-box";
        versionBox.appendChild(versionSelector);
        nav.appendChild(versionBox); // Append to the .right-buttons container
      } else {
        console.error(".right-buttons element not found.");
      }
    })
    .catch((error) => {
      console.error(
        "There has been a problem with your fetch operation:",
        error,
      );
    });
});
