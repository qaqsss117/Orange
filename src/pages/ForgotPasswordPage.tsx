import { type FormEvent, useEffect, useId, useState } from "react";
import {
  AlertTriangle,
  BadgeCheck,
  Eye,
  EyeOff,
  LoaderCircle,
  LockKeyhole,
  Mail,
  Send,
  ShieldCheck,
} from "lucide-react";
import { Link, useNavigate } from "react-router-dom";
import type { ConfigResponse } from "../businessApi";
import {
  parseResetPasswordCommandRequest,
  parseSendEmailVerificationCommandRequest,
} from "../ipc";
import { SHELL_TEXT } from "../shellContent";
import { type ShellServices, toPublicUiError } from "../shellServices";

type FieldName = "email" | "password" | "confirmPassword" | "emailCode";
type FieldErrors = Partial<Record<FieldName, string>>;

interface ForgotPasswordPageProps {
  config: ConfigResponse;
  services: ShellServices;
  onCompleted: () => void;
}

export function ForgotPasswordPage({
  config,
  services,
  onCompleted,
}: ForgotPasswordPageProps) {
  const navigate = useNavigate();
  const emailInputId = useId();
  const emailErrorId = useId();
  const codeInputId = useId();
  const codeErrorId = useId();
  const codeHintId = useId();
  const passwordInputId = useId();
  const passwordErrorId = useId();
  const confirmInputId = useId();
  const confirmErrorId = useId();
  const [email, setEmail] = useState("");
  const [emailCode, setEmailCode] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [verificationBusy, setVerificationBusy] = useState(false);
  const [verificationCooldown, setVerificationCooldown] = useState(0);
  const [verificationSent, setVerificationSent] = useState(false);
  const [verificationError, setVerificationError] = useState<string | null>(
    null,
  );
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({});
  const [serviceError, setServiceError] = useState<string | null>(null);
  const unavailable = config.maintenance;

  const clearFieldError = (field: FieldName) => {
    setFieldErrors((current) => ({ ...current, [field]: undefined }));
    setServiceError(null);
  };

  useEffect(() => {
    if (verificationCooldown === 0) return;
    const timeout = window.setTimeout(
      () => setVerificationCooldown((current) => Math.max(0, current - 1)),
      1_000,
    );
    return () => window.clearTimeout(timeout);
  }, [verificationCooldown]);

  const sendVerificationCode = async () => {
    if (busy || verificationBusy || verificationCooldown > 0 || unavailable) {
      return;
    }
    setVerificationError(null);
    setServiceError(null);
    try {
      parseSendEmailVerificationCommandRequest({ email });
    } catch (error) {
      setFieldErrors({ email: toPublicUiError(error).message });
      return;
    }

    setVerificationBusy(true);
    try {
      const response = await services.sendEmailVerification(email);
      if (!response.sent) {
        setVerificationError(SHELL_TEXT.operationFailed);
        return;
      }
      setVerificationSent(true);
      setVerificationCooldown(60);
    } catch (error) {
      setVerificationError(toPublicUiError(error).message);
    } finally {
      setVerificationBusy(false);
    }
  };

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy || verificationBusy || unavailable) return;

    setFieldErrors({});
    setServiceError(null);
    if (password !== confirmPassword) {
      setFieldErrors({ confirmPassword: SHELL_TEXT.passwordMismatch });
      return;
    }

    try {
      parseResetPasswordCommandRequest({ email, password, emailCode });
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
      const response = await services.resetPassword({
        email,
        password,
        emailCode,
      });
      if (!response.succeeded) {
        setServiceError(SHELL_TEXT.operationFailed);
        return;
      }
      setPassword("");
      setConfirmPassword("");
      setEmailCode("");
      onCompleted();
      navigate("/login", { replace: true });
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
      <section className="auth-panel" aria-labelledby="forgot-password-title">
        <div className="auth-heading">
          <span className="auth-security-state">
            <ShieldCheck aria-hidden="true" />
            {SHELL_TEXT.serviceReady}
          </span>
          <h1 id="forgot-password-title">{SHELL_TEXT.forgotPasswordTitle}</h1>
          <p>{SHELL_TEXT.forgotPasswordSubtitle}</p>
        </div>

        {unavailable && (
          <div className="inline-state state-warning" role="alert">
            <AlertTriangle aria-hidden="true" />
            <div>
              <strong>{SHELL_TEXT.maintenanceTitle}</strong>
              <span>{config.notice ?? SHELL_TEXT.maintenanceDetail}</span>
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
                disabled={busy || verificationBusy || unavailable}
                aria-invalid={fieldErrors.email ? "true" : undefined}
                aria-describedby={fieldErrors.email ? emailErrorId : undefined}
                onChange={(event) => {
                  setEmail(event.target.value);
                  setEmailCode("");
                  setVerificationSent(false);
                  setVerificationError(null);
                  setFieldErrors((current) => ({
                    ...current,
                    email: undefined,
                    emailCode: undefined,
                  }));
                  setServiceError(null);
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
            <label htmlFor={codeInputId}>{SHELL_TEXT.emailCode}</label>
            <span className="input-shell verification-input-shell">
              <BadgeCheck aria-hidden="true" />
              <input
                id={codeInputId}
                type="text"
                name="emailCode"
                inputMode="numeric"
                autoComplete="one-time-code"
                pattern="[0-9]{6}"
                maxLength={6}
                placeholder={SHELL_TEXT.emailCodePlaceholder}
                value={emailCode}
                disabled={busy || unavailable}
                required
                aria-invalid={
                  fieldErrors.emailCode || verificationError
                    ? "true"
                    : undefined
                }
                aria-describedby={
                  fieldErrors.emailCode || verificationError
                    ? codeErrorId
                    : verificationSent
                      ? codeHintId
                      : undefined
                }
                onChange={(event) => {
                  setEmailCode(event.target.value);
                  setVerificationError(null);
                  clearFieldError("emailCode");
                }}
              />
              <button
                type="button"
                className="verification-send-button"
                disabled={
                  busy ||
                  verificationBusy ||
                  verificationCooldown > 0 ||
                  unavailable ||
                  email.trim() === ""
                }
                onClick={() => void sendVerificationCode()}
              >
                {verificationBusy ? (
                  <LoaderCircle className="spinning" aria-hidden="true" />
                ) : verificationCooldown === 0 ? (
                  <Send aria-hidden="true" />
                ) : null}
                {verificationBusy
                  ? SHELL_TEXT.sendingEmailCode
                  : verificationCooldown > 0
                    ? `${verificationCooldown} 秒`
                    : SHELL_TEXT.sendEmailCode}
              </button>
            </span>
            {(fieldErrors.emailCode || verificationError) && (
              <span id={codeErrorId} className="field-error" role="alert">
                {fieldErrors.emailCode ?? verificationError}
              </span>
            )}
            {!fieldErrors.emailCode &&
              !verificationError &&
              verificationSent && (
                <span id={codeHintId} className="field-hint">
                  {SHELL_TEXT.emailCodeSent}
                </span>
              )}
          </div>

          <div className="field-group">
            <label htmlFor={passwordInputId}>{SHELL_TEXT.newPassword}</label>
            <span className="input-shell">
              <LockKeyhole aria-hidden="true" />
              <input
                id={passwordInputId}
                type={passwordVisible ? "text" : "password"}
                name="password"
                autoComplete="new-password"
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

          <div className="field-group">
            <label htmlFor={confirmInputId}>{SHELL_TEXT.confirmPassword}</label>
            <span className="input-shell">
              <LockKeyhole aria-hidden="true" />
              <input
                id={confirmInputId}
                type={passwordVisible ? "text" : "password"}
                name="confirmPassword"
                autoComplete="new-password"
                value={confirmPassword}
                disabled={busy || unavailable}
                aria-invalid={fieldErrors.confirmPassword ? "true" : undefined}
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
              <span id={confirmErrorId} className="field-error" role="alert">
                {fieldErrors.confirmPassword}
              </span>
            )}
          </div>

          {serviceError && (
            <div className="form-error" role="alert">
              <AlertTriangle aria-hidden="true" />
              <span>{serviceError}</span>
            </div>
          )}

          <button
            type="submit"
            className="primary-action auth-submit"
            disabled={busy || verificationBusy || unavailable}
          >
            {busy && <LoaderCircle className="spinning" aria-hidden="true" />}
            {busy ? SHELL_TEXT.resettingPassword : SHELL_TEXT.resetPassword}
          </button>
        </form>

        <div className="auth-switch">
          <Link to="/login">{SHELL_TEXT.backToLogin}</Link>
        </div>
      </section>
    </main>
  );
}
