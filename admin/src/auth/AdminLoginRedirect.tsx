import { Navigate, useLocation } from 'react-router-dom';

import { loginDestination } from './loginDestination';

export function AdminLoginRedirect() {
  const location = useLocation();
  const destination = loginDestination(location.state, window.location.origin);

  return (
    <Navigate
      replace
      to={`/app/login?redirect=${encodeURIComponent(destination)}`}
    />
  );
}
