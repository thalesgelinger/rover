(function () {
  function setupMobileMenu() {
    var button = document.getElementById("mobile-menu-button");
    var menu = document.getElementById("mobile-menu");
    var menuIcon = document.getElementById("menu-icon");
    var closeIcon = document.getElementById("close-icon");

    if (!button || !menu || !menuIcon || !closeIcon) {
      return;
    }

    var links = menu.querySelectorAll("a");
    var open = false;

    function render() {
      menu.classList.toggle("hidden", !open);
      menuIcon.classList.toggle("hidden", open);
      closeIcon.classList.toggle("hidden", !open);
      button.setAttribute("aria-expanded", open ? "true" : "false");
      button.setAttribute("aria-label", open ? "Close menu" : "Open menu");
    }

    button.addEventListener("click", function () {
      open = !open;
      render();
    });

    links.forEach(function (link) {
      link.addEventListener("click", function () {
        open = false;
        render();
      });
    });

    render();
  }

  function renderIcons() {
    if (window.lucide && typeof window.lucide.createIcons === "function") {
      window.lucide.createIcons();
    }
  }

  setupMobileMenu();
  renderIcons();
})();
