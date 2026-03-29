/* @refresh reload */
import { render } from "solid-js/web";
import { Route, Router } from "@solidjs/router";
import { AuthProvider } from "./context/auth";
import App from "./App";
import Dashboard from "./pages/Dashboard";
import ServerDetail from "./pages/ServerDetail";
import CreateServer from "./pages/CreateServer";
import Templates from "./pages/Templates";
import SystemHealth from "./pages/SystemHealth";
import Login from "./pages/Login";
import Register from "./pages/Register";
import Setup from "./pages/Setup";
import AdminPanel from "./pages/AdminPanel";
import NotFound from "./pages/NotFound";

const root = document.getElementById("root");

if (!root) {
  throw new Error("Root element #root not found in the document");
}

render(
  () => (
    <AuthProvider>
      <Router root={App}>
        <Route path="/" component={Dashboard} />
        <Route path="/create" component={CreateServer} />
        <Route path="/server/:id" component={ServerDetail} />
        <Route path="/templates" component={Templates} />
        <Route path="/health" component={SystemHealth} />
        <Route path="/login" component={Login} />
        <Route path="/register" component={Register} />
        <Route path="/setup" component={Setup} />
        <Route path="/admin" component={AdminPanel} />
        <Route path="*" component={NotFound} />
      </Router>
    </AuthProvider>
  ),
  root,
);
