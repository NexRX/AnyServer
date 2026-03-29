import {
  type Component,
  type ParentProps,
  Show,
  ErrorBoundary,
  createSignal,
  onCleanup,
  createEffect,
  on,
} from "solid-js";
import { A, useNavigate, useLocation } from "@solidjs/router";
import { useAuth } from "./context/auth";
import Loader from "./components/Loader";
import GlobalErrorFallback from "./components/GlobalErrorFallback";
import RateLimitBanner from "./components/RateLimitBanner";

const App: Component<ParentProps> = (props) => {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();

  const [mobileMenuOpen, setMobileMenuOpen] = createSignal(false);

  // Close mobile menu on route change
  createEffect(() => {
    location.pathname;
    setMobileMenuOpen(false);
  });

  // Close mobile menu on escape key
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && mobileMenuOpen()) {
      setMobileMenuOpen(false);
    }
  };

  if (typeof document !== "undefined") {
    document.addEventListener("keydown", handleKeyDown);
    onCleanup(() => document.removeEventListener("keydown", handleKeyDown));
  }

  // Prevent body scroll when mobile menu is open
  createEffect(() => {
    if (typeof document !== "undefined") {
      if (mobileMenuOpen()) {
        document.body.style.overflow = "hidden";
      } else {
        document.body.style.overflow = "";
      }
    }
  });

  const handleLogout = () => {
    setMobileMenuOpen(false);
    auth.logout();
    navigate("/login", { replace: true });
  };

  const isPublicRoute = () => {
    const path = location.pathname;
    return path === "/login" || path === "/register" || path === "/setup";
  };

  // Show the loading spinner on protected routes until the auth state is
  // fully resolved.  `auth.loading()` alone isn't sufficient — settings
  // might still be null (e.g. getAuthStatus was retrying) even after
  // loading flips to false.  Without this guard the dashboard flashes
  // briefly (with no navbar) before shouldRedirect navigates to /login.
  const isLoading = () =>
    !isPublicRoute() && (auth.loading() || auth.settings() === null);

  const shouldRedirect = () => {
    if (auth.loading()) return false;

    // If settings haven't loaded yet (e.g. getAuthStatus was aborted by a
    // hard refresh), we don't know whether setup is complete.  Treat this
    // the same as "still loading" — don't redirect anywhere until we have
    // a definitive answer.
    if (auth.settings() === null) return false;

    const path = location.pathname;

    // Redirect away from public routes that are no longer appropriate:
    // - /setup when setup is already complete
    // - /login or /register when already logged in
    if (isPublicRoute()) {
      if (path === "/setup" && auth.isSetupComplete()) return "login";
      if ((path === "/login" || path === "/register") && auth.isLoggedIn())
        return "";
      return false;
    }

    if (!auth.isSetupComplete()) return "setup";
    if (!auth.isLoggedIn()) return "login";
    return false;
  };

  // Perform redirects in a proper effect rather than as a side-effect
  // inside a <Show when={}> computation.  This ensures the navigate()
  // call only fires when the redirect target actually changes, and
  // avoids racing with the auth context while it is still loading.
  createEffect(
    on(shouldRedirect, (target) => {
      if (target !== false) {
        navigate(`/${target}`, { replace: true });
      }
    }),
  );

  const toggleMobileMenu = () => {
    setMobileMenuOpen((prev) => !prev);
  };

  return (
    <div class="app">
      <RateLimitBanner belowNavbar={!isPublicRoute() && auth.isLoggedIn()} />
      <Show when={!isPublicRoute() && auth.isLoggedIn()}>
        <nav class="navbar" role="navigation" aria-label="Main navigation">
          <A href="/" class="logo" aria-label="AnyServer — Go to dashboard">
            ⚡ AnyServer
          </A>

          {/* Desktop nav links */}
          <div class="nav-links nav-links-desktop">
            <A href="/" end activeClass="active">
              Dashboard
            </A>
            <A href="/templates" activeClass="active">
              Templates
            </A>
            <A href="/health" activeClass="active">
              Health
            </A>
            <Show when={auth.isAdmin()}>
              <A href="/admin" activeClass="active">
                Admin
              </A>
            </Show>
          </div>

          {/* Desktop user section */}
          <div class="nav-user nav-user-desktop">
            <span
              class="nav-username"
              title={`Logged in as ${auth.user()?.username ?? ""}`}
            >
              <span class="nav-user-icon" aria-hidden="true">
                👤
              </span>
              {auth.user()?.username ?? ""}
              <Show when={auth.isAdmin()}>
                <span class="nav-role-badge">admin</span>
              </Show>
            </span>
            <button class="btn btn-sm nav-logout" onClick={handleLogout}>
              Sign Out
            </button>
          </div>

          {/* Mobile hamburger button */}
          <button
            class="nav-hamburger"
            onClick={toggleMobileMenu}
            aria-label={mobileMenuOpen() ? "Close menu" : "Open menu"}
            aria-expanded={mobileMenuOpen()}
            aria-controls="mobile-nav-menu"
          >
            <span
              class="hamburger-icon"
              classList={{ open: mobileMenuOpen() }}
              aria-hidden="true"
            >
              <span class="hamburger-bar" />
              <span class="hamburger-bar" />
              <span class="hamburger-bar" />
            </span>
          </button>
        </nav>

        {/* Mobile backdrop */}
        <Show when={mobileMenuOpen()}>
          <div
            class="mobile-nav-backdrop"
            onClick={() => setMobileMenuOpen(false)}
            aria-hidden="true"
          />
        </Show>

        {/* Mobile slide-out menu */}
        <div
          id="mobile-nav-menu"
          class="mobile-nav-menu"
          classList={{ open: mobileMenuOpen() }}
          aria-hidden={!mobileMenuOpen()}
        >
          <div class="mobile-nav-user">
            <span class="mobile-nav-user-icon" aria-hidden="true">
              👤
            </span>
            <div class="mobile-nav-user-info">
              <span class="mobile-nav-username">
                {auth.user()?.username ?? ""}
              </span>
              <Show when={auth.isAdmin()}>
                <span class="nav-role-badge">admin</span>
              </Show>
            </div>
          </div>

          <div class="mobile-nav-divider" />

          <div class="mobile-nav-links">
            <A
              href="/"
              end
              activeClass="active"
              class="mobile-nav-link"
              onClick={() => setMobileMenuOpen(false)}
            >
              <span class="mobile-nav-link-icon" aria-hidden="true">
                📊
              </span>
              Dashboard
            </A>
            <A
              href="/templates"
              activeClass="active"
              class="mobile-nav-link"
              onClick={() => setMobileMenuOpen(false)}
            >
              <span class="mobile-nav-link-icon" aria-hidden="true">
                📋
              </span>
              Templates
            </A>
            <A
              href="/health"
              activeClass="active"
              class="mobile-nav-link"
              onClick={() => setMobileMenuOpen(false)}
            >
              <span class="mobile-nav-link-icon" aria-hidden="true">
                💚
              </span>
              Health
            </A>
            <Show when={auth.isAdmin()}>
              <A
                href="/admin"
                activeClass="active"
                class="mobile-nav-link"
                onClick={() => setMobileMenuOpen(false)}
              >
                <span class="mobile-nav-link-icon" aria-hidden="true">
                  ⚙️
                </span>
                Admin
              </A>
            </Show>
          </div>

          <div class="mobile-nav-divider" />

          <button class="mobile-nav-logout" onClick={handleLogout}>
            Sign Out
          </button>
        </div>
      </Show>

      <ErrorBoundary
        fallback={(err, reset) => (
          <GlobalErrorFallback error={err} reset={reset} />
        )}
      >
        <Show
          when={!isLoading()}
          fallback={
            <main class="content">
              <Loader />
            </main>
          }
        >
          <Show
            when={!shouldRedirect()}
            fallback={
              <main class="content">
                <Loader message="Redirecting" />
              </main>
            }
          >
            <main
              class={isPublicRoute() ? "content content-fullscreen" : "content"}
            >
              {props.children}
            </main>
          </Show>
        </Show>
      </ErrorBoundary>
    </div>
  );
};

export default App;
