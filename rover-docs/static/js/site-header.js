(function () {
  function setupDocsMobileSectionToggle() {
    var isMobile = window.matchMedia("(max-width: 767.98px)").matches;
    if (!isMobile) {
      return;
    }

    var sidebarSearch = document.querySelector(".td-sidebar__search");
    var sidebarToggle = document.querySelector(".td-sidebar__toggle");
    var navbarList = document.querySelector("#main_navbar .navbar-nav");

    if (!sidebarSearch || !sidebarToggle || !navbarList) {
      return;
    }

    if (document.querySelector(".td-navbar__section-toggle")) {
      return;
    }

    var wrapper = document.createElement("li");
    wrapper.className = "nav-item td-navbar__section-toggle";
    wrapper.appendChild(sidebarToggle);
    navbarList.appendChild(wrapper);
    sidebarSearch.classList.add("td-sidebar__search--moved");
  }

  function renderIcons() {
    if (window.lucide && typeof window.lucide.createIcons === "function") {
      window.lucide.createIcons();
    }
  }

  setupDocsMobileSectionToggle();
  renderIcons();
})();
