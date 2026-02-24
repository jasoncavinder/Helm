(function () {
  var menuId = "download-helm-menu";
  var triggers = Array.prototype.slice.call(
    document.querySelectorAll('a[href="#' + menuId + '"]'),
  );
  var menu = document.getElementById(menuId);
  var cliInstallModal = document.getElementById("cli-install-modal");
  var commandElement = document.getElementById("cli-install-command");

  if (!menu || triggers.length === 0 || !commandElement) {
    return;
  }

  var openCliModalButton = menu.querySelector("[data-open-cli-install-modal]");
  var copyButton = cliInstallModal
    ? cliInstallModal.querySelector("[data-copy-cli-install-command]")
    : null;
  var copyStatus = cliInstallModal
    ? cliInstallModal.querySelector("[data-copy-status]")
    : null;
  var activeTrigger = null;

  function closeMenu() {
    menu.hidden = true;
    if (activeTrigger) {
      activeTrigger.setAttribute("aria-expanded", "false");
    }
    activeTrigger = null;
  }

  function openMenu(trigger) {
    var rect = trigger.getBoundingClientRect();
    menu.style.left = Math.max(12, rect.left) + "px";
    menu.style.top = rect.bottom + 8 + "px";
    menu.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    activeTrigger = trigger;
  }

  triggers.forEach(function (trigger) {
    trigger.setAttribute("aria-haspopup", "menu");
    trigger.setAttribute("aria-expanded", "false");

    trigger.addEventListener("click", function (event) {
      event.preventDefault();
      if (!menu.hidden && activeTrigger === trigger) {
        closeMenu();
        return;
      }

      closeMenu();
      openMenu(trigger);
    });
  });

  document.addEventListener("click", function (event) {
    var target = event.target;
    if (!(target instanceof Node) || menu.hidden) {
      return;
    }

    if (
      menu.contains(target) ||
      triggers.some(function (trigger) {
        return trigger.contains(target);
      })
    ) {
      return;
    }

    closeMenu();
  });

  document.addEventListener("keydown", function (event) {
    if (event.key === "Escape") {
      closeMenu();
    }
  });

  if (openCliModalButton) {
    openCliModalButton.addEventListener("click", function () {
      closeMenu();
      if (cliInstallModal && typeof cliInstallModal.showModal === "function") {
        if (copyStatus) {
          copyStatus.textContent = "";
        }
        cliInstallModal.showModal();
      }
    });
  }

  if (copyButton) {
    copyButton.addEventListener("click", async function () {
      var commandText = commandElement.textContent
        ? commandElement.textContent.trim()
        : "";
      if (!commandText) {
        return;
      }

      try {
        await navigator.clipboard.writeText(commandText);
        if (copyStatus) {
          copyStatus.textContent = "Command copied.";
        }
      } catch (_error) {
        var fallback = document.createElement("textarea");
        fallback.value = commandText;
        fallback.setAttribute("readonly", "readonly");
        fallback.style.position = "fixed";
        fallback.style.opacity = "0";
        document.body.appendChild(fallback);
        fallback.select();
        document.execCommand("copy");
        document.body.removeChild(fallback);
        if (copyStatus) {
          copyStatus.textContent = "Command copied.";
        }
      }
    });
  }
})();
