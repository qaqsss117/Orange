import { type FormEvent, useEffect, useId, useState } from "react";
import {
  AlertTriangle,
  BadgeCheck,
  Eye,
  EyeOff,
  ExternalLink,
  LoaderCircle,
  LockKeyhole,
  Mail,
  RotateCw,
  Send,
  ShieldCheck,
  Ticket,
} from "lucide-react";
import { Link } from "react-router-dom";
import type { ConfigResponse, UserProfile } from "../businessApi";
import {
  parseLoginCommandRequest,
  parseRegisterCommandRequest,
  parseSendEmailVerificationCommandRequest,
  type LegalDocument,
} from "../ipc";
import { SHELL_TEXT } from "../shellContent";
import { type ShellServices, toPublicUiError } from "../shellServices";

type AuthMode = "login" | "register";
type FieldName =
  "email" | "password" | "confirmPassword" | "emailCode" | "inviteCode";
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
  const emailCodeErrorId = useId();
  const emailCodeHintId = useId();
  const inviteErrorId = useId();
  const agreementErrorId = useId();
  const emailInputId = useId();
  const passwordInputId = useId();
  const confirmInputId = useId();
  const emailCodeInputId = useId();
  const inviteInputId = useId();
  const agreementInputId = useId();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [emailCode, setEmailCode] = useState("");
  const [inviteCode, setInviteCode] = useState("");
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
  const [agreementAccepted, setAgreementAccepted] = useState(false);
  const [agreementError, setAgreementError] = useState<string | null>(null);
  const [legalDocumentPending, setLegalDocumentPending] =
    useState<LegalDocument | null>(null);
  const [legalDocumentError, setLegalDocumentError] = useState<string | null>(
    null,
  );
  const isRegister = mode === "register";
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
      const publicError = toPublicUiError(error);
      setFieldErrors({ email: publicError.message });
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

  const openLegalDocument = async (document: LegalDocument) => {
    if (legalDocumentPending !== null) return;
    setLegalDocumentPending(document);
    setLegalDocumentError(null);
    try {
      await services.openLegalDocument(document);
    } catch (error) {
      setLegalDocumentError(toPublicUiError(error).message);
    } finally {
      setLegalDocumentPending(null);
    }
  };

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy || verificationBusy || unavailable) {
      return;
    }

    setFieldErrors({});
    setServiceError(null);
    setAgreementError(null);
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
          emailCode: config.registrationRequiresEmailVerification
            ? emailCode
            : null,
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
    if (!agreementAccepted) {
      setAgreementError(SHELL_TEXT.agreementRequired);
      return;
    }

    setBusy(true);
    try {
      const response = isRegister
        ? await services.register({
            email,
            password,
            emailCode: config.registrationRequiresEmailVerification
              ? emailCode
              : null,
            inviteCode: inviteCode === "" ? null : inviteCode,
          })
        : await services.login({ email, password });
      if (!response.authenticated) {
        setServiceError(SHELL_TEXT.operationFailed);
        return;
      }
      setPassword("");
      setConfirmPassword("");
      setEmailCode("");
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

              {config.registrationRequiresEmailVerification && (
                <div className="field-group">
                  <label htmlFor={emailCodeInputId}>
                    {SHELL_TEXT.emailCode}
                  </label>
                  <span className="input-shell verification-input-shell">
                    <BadgeCheck aria-hidden="true" />
                    <input
                      id={emailCodeInputId}
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
                          ? emailCodeErrorId
                          : verificationSent
                            ? emailCodeHintId
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
                    <span
                      id={emailCodeErrorId}
                      className="field-error"
                      role="alert"
                    >
                      {fieldErrors.emailCode ?? verificationError}
                    </span>
                  )}
                  {!fieldErrors.emailCode &&
                    !verificationError &&
                    verificationSent && (
                      <span id={emailCodeHintId} className="field-hint">
                        {SHELL_TEXT.emailCodeSent}
                      </span>
                    )}
                </div>
              )}

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

          <div className="auth-agreement">
            <input
              id={agreementInputId}
              type="checkbox"
              checked={agreementAccepted}
              disabled={busy || unavailable}
              aria-invalid={agreementError === null ? undefined : "true"}
              aria-describedby={
                agreementError === null ? undefined : agreementErrorId
              }
              onChange={(event) => {
                setAgreementAccepted(event.target.checked);
                setAgreementError(null);
              }}
            />
            <div className="auth-agreement-copy">
              <label htmlFor={agreementInputId}>
                {SHELL_TEXT.agreementLabel}
              </label>
              <button
                type="button"
                className="auth-legal-action"
                disabled={busy || legalDocumentPending !== null}
                aria-busy={legalDocumentPending === "terms_of_service"}
                onClick={() => void openLegalDocument("terms_of_service")}
              >
                {legalDocumentPending === "terms_of_service"
                  ? "正在打开"
                  : SHELL_TEXT.termsOfService}
                <ExternalLink aria-hidden="true" />
              </button>
              <span>和</span>
              <button
                type="button"
                className="auth-legal-action"
                disabled={busy || legalDocumentPending !== null}
                aria-busy={legalDocumentPending === "privacy_policy"}
                onClick={() => void openLegalDocument("privacy_policy")}
              >
                {legalDocumentPending === "privacy_policy"
                  ? "正在打开"
                  : SHELL_TEXT.privacyPolicy}
                <ExternalLink aria-hidden="true" />
              </button>
            </div>
          </div>

          {agreementError !== null && (
            <span id={agreementErrorId} className="field-error" role="alert">
              {agreementError}
            </span>
          )}

          {legalDocumentError !== null && (
            <div className="form-error" role="alert">
              <AlertTriangle aria-hidden="true" />
              <span>{legalDocumentError}</span>
            </div>
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
            disabled={busy || verificationBusy || unavailable}
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
          {!isRegister && (
            <Link to="/forgot-password">{SHELL_TEXT.forgotPassword}</Link>
          )}
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
