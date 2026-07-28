import { type FormEvent, useId, useState } from "react";
import {
  AlertTriangle,
  Eye,
  EyeOff,
  LoaderCircle,
  LockKeyhole,
  Mail,
  RotateCw,
  ShieldCheck,
  Ticket,
} from "lucide-react";
import { Link } from "react-router-dom";
import type { ConfigResponse, UserProfile } from "../businessApi";
import { parseLoginCommandRequest, parseRegisterCommandRequest } from "../ipc";
import { SHELL_TEXT } from "../shellContent";
import { type ShellServices, toPublicUiError } from "../shellServices";

type AuthMode = "login" | "register";
type FieldName = "email" | "password" | "confirmPassword" | "inviteCode";
type FieldErrors = Partial<Record<FieldName, string>>;

interface AuthPageProps {
  mode: AuthMode;
  config: ConfigResponse;
  unverified: boolean;
  services: ShellServices;
  onAuthenticated: (user: UserProfile, message: string) => void;
  onRetryInitialization: () => void;
}

export function AuthPage({
  mode,
  config,
  unverified,
  services,
  onAuthenticated,
  onRetryInitialization,
}: AuthPageProps) {
  const emailErrorId = useId();
  const passwordErrorId = useId();
  const confirmErrorId = useId();
  const inviteErrorId = useId();
  const emailInputId = useId();
  const passwordInputId = useId();
  const confirmInputId = useId();
  const inviteInputId = useId();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [inviteCode, setInviteCode] = useState("");
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({});
  const [serviceError, setServiceError] = useState<string | null>(null);
  const isRegister = mode === "register";
  const unavailable = config.maintenance;

  const clearFieldError = (field: FieldName) => {
    setFieldErrors((current) => ({ ...current, [field]: undefined }));
    setServiceError(null);
  };

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy || unavailable) {
      return;
    }

    setFieldErrors({});
    setServiceError(null);
    if (isRegister && password !== confirmPassword) {
      setFieldErrors({ confirmPassword: SHELL_TEXT.passwordMismatch });
      return;
    }
    if (isRegister && config.registrationRequiresInvite && inviteCode === "") {
      setFieldErrors({ inviteCode: SHELL_TEXT.inviteRequired });
      return;
    }

    try {
      if (isRegister) {
        parseRegisterCommandRequest({
          email,
          password,
          inviteCode: inviteCode === "" ? null : inviteCode,
        });
      } else {
        parseLoginCommandRequest({ email, password });
      }
    } catch (error) {
      const publicError = toPublicUiError(error);
      if (publicError.field !== null) {
        setFieldErrors({ [publicError.field]: publicError.message });
      } else {
        setServiceError(publicError.message);
      }
      return;
    }

    setBusy(true);
    try {
      const response = isRegister
        ? await services.register({
            email,
            password,
            inviteCode: inviteCode === "" ? null : inviteCode,
          })
        : await services.login({ email, password });
      if (!response.authenticated) {
        setServiceError(SHELL_TEXT.operationFailed);
        return;
      }
      setPassword("");
      setConfirmPassword("");
      onAuthenticated(
        response.user,
        isRegister ? SHELL_TEXT.registerSuccess : SHELL_TEXT.loginSuccess,
      );
    } catch (error) {
      const publicError = toPublicUiError(error);
      if (publicError.field !== null) {
        setFieldErrors({ [publicError.field]: publicError.message });
      } else {
        setServiceError(publicError.message);
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="auth-layout">
      <section className="auth-panel" aria-labelledby="auth-title">
        <div className="auth-heading">
          <span className="auth-security-state">
            <ShieldCheck aria-hidden="true" />
            {SHELL_TEXT.serviceReady}
          </span>
          <h1 id="auth-title">
            {isRegister ? SHELL_TEXT.registerTitle : SHELL_TEXT.loginTitle}
          </h1>
          <p>
            {isRegister
              ? SHELL_TEXT.registerSubtitle
              : SHELL_TEXT.loginSubtitle}
          </p>
        </div>

        {config.maintenance && (
          <div className="inline-state state-warning" role="alert">
            <AlertTriangle aria-hidden="true" />
            <div>
              <strong>{SHELL_TEXT.maintenanceTitle}</strong>
              <span>{config.notice ?? SHELL_TEXT.maintenanceDetail}</span>
            </div>
          </div>
        )}

        {unverified && (
          <div className="inline-state state-warning" role="alert">
            <AlertTriangle aria-hidden="true" />
            <div>
              <strong>{SHELL_TEXT.unverifiedTitle}</strong>
              <span>{SHELL_TEXT.unverifiedDetail}</span>
              <button
                type="button"
                className="inline-action"
                onClick={onRetryInitialization}
              >
                <RotateCw aria-hidden="true" />
                {SHELL_TEXT.retryVerification}
              </button>
            </div>
          </div>
        )}

        <form className="auth-form" noValidate onSubmit={handleSubmit}>
          <div className="field-group">
            <label htmlFor={emailInputId}>{SHELL_TEXT.email}</label>
            <span className="input-shell">
              <Mail aria-hidden="true" />
              <input
                id={emailInputId}
                type="email"
                name="email"
                inputMode="email"
                autoComplete="email"
                placeholder={SHELL_TEXT.emailPlaceholder}
                value={email}
                disabled={busy || unavailable}
                aria-invalid={fieldErrors.email ? "true" : undefined}
                aria-describedby={fieldErrors.email ? emailErrorId : undefined}
                onChange={(event) => {
                  setEmail(event.target.value);
                  clearFieldError("email");
                }}
              />
            </span>
            {fieldErrors.email && (
              <span id={emailErrorId} className="field-error" role="alert">
                {fieldErrors.email}
              </span>
            )}
          </div>

          <div className="field-group">
            <label htmlFor={passwordInputId}>{SHELL_TEXT.password}</label>
            <span className="input-shell">
              <LockKeyhole aria-hidden="true" />
              <input
                id={passwordInputId}
                type={passwordVisible ? "text" : "password"}
                name="password"
                autoComplete={isRegister ? "new-password" : "current-password"}
                value={password}
                disabled={busy || unavailable}
                aria-invalid={fieldErrors.password ? "true" : undefined}
                aria-describedby={
                  fieldErrors.password ? passwordErrorId : undefined
                }
                onChange={(event) => {
                  setPassword(event.target.value);
                  clearFieldError("password");
                }}
              />
              <button
                type="button"
                className="password-toggle"
                aria-label={
                  passwordVisible
                    ? SHELL_TEXT.hidePassword
                    : SHELL_TEXT.showPassword
                }
                title={
                  passwordVisible
                    ? SHELL_TEXT.hidePassword
                    : SHELL_TEXT.showPassword
                }
                disabled={busy || unavailable}
                onClick={() => setPasswordVisible((visible) => !visible)}
              >
                {passwordVisible ? (
                  <EyeOff aria-hidden="true" />
                ) : (
                  <Eye aria-hidden="true" />
                )}
              </button>
            </span>
            {fieldErrors.password && (
              <span id={passwordErrorId} className="field-error" role="alert">
                {fieldErrors.password}
              </span>
            )}
          </div>

          {isRegister && (
            <>
              <div className="field-group">
                <label htmlFor={confirmInputId}>
                  {SHELL_TEXT.confirmPassword}
                </label>
                <span className="input-shell">
                  <LockKeyhole aria-hidden="true" />
                  <input
                    id={confirmInputId}
                    type={passwordVisible ? "text" : "password"}
                    name="confirmPassword"
                    autoComplete="new-password"
                    value={confirmPassword}
                    disabled={busy || unavailable}
                    aria-invalid={
                      fieldErrors.confirmPassword ? "true" : undefined
                    }
                    aria-describedby={
                      fieldErrors.confirmPassword ? confirmErrorId : undefined
                    }
                    onChange={(event) => {
                      setConfirmPassword(event.target.value);
                      clearFieldError("confirmPassword");
                    }}
                  />
                </span>
                {fieldErrors.confirmPassword && (
                  <span
                    id={confirmErrorId}
                    className="field-error"
                    role="alert"
                  >
                    {fieldErrors.confirmPassword}
                  </span>
                )}
              </div>

              <div className="field-group">
                <label htmlFor={inviteInputId}>
                  {config.registrationRequiresInvite
                    ? SHELL_TEXT.inviteCode
                    : SHELL_TEXT.inviteOptional}
                </label>
                <span className="input-shell">
                  <Ticket aria-hidden="true" />
                  <input
                    id={inviteInputId}
                    type="text"
                    name="inviteCode"
                    autoComplete="off"
                    value={inviteCode}
                    disabled={busy || unavailable}
                    required={config.registrationRequiresInvite}
                    aria-invalid={fieldErrors.inviteCode ? "true" : undefined}
                    aria-describedby={
                      fieldErrors.inviteCode ? inviteErrorId : undefined
                    }
                    onChange={(event) => {
                      setInviteCode(event.target.value);
                      clearFieldError("inviteCode");
                    }}
                  />
                </span>
                {fieldErrors.inviteCode && (
                  <span id={inviteErrorId} className="field-error" role="alert">
                    {fieldErrors.inviteCode}
                  </span>
                )}
              </div>
            </>
          )}

          {serviceError && (
            <div className="form-error" role="alert">
              <AlertTriangle aria-hidden="true" />
              <span>{serviceError}</span>
            </div>
          )}

          <button
            type="submit"
            className="primary-action auth-submit"
            disabled={busy || unavailable}
          >
            {busy && <LoaderCircle className="spinning" aria-hidden="true" />}
            {busy
              ? isRegister
                ? SHELL_TEXT.registering
                : SHELL_TEXT.loggingIn
              : isRegister
                ? SHELL_TEXT.register
                : SHELL_TEXT.login}
          </button>
        </form>

        <div className="auth-switch">
          <Link to={isRegister ? "/login" : "/register"}>
            {isRegister
              ? SHELL_TEXT.alreadyRegistered
              : SHELL_TEXT.createAccount}
          </Link>
        </div>
      </section>
    </main>
  );
}
