import { t, useLocale } from "./locale";
import { useEffect, isValidElement, cloneElement, Fragment } from "react";
import { useEntrance } from "./motion";
import type { ReactNode, ButtonHTMLAttributes, Ref } from "react";
import { X } from "lucide-react";
export function Button({
  children,
  icon,
  title,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { icon?: ReactNode;ref?:Ref<HTMLButtonElement> }) {
  useLocale();
  return (
    <button type="button" title={title} {...props}>
      {icon}
      {children}
    </button>
  );
}
export function Panel({
  title,
  actions,
  children,
  className = "",
}: {
  title: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  useLocale();
  return (
    <section className={`panel ${className}`} aria-label={title}>
      <div className="panel-heading">
        <h2>{title}</h2>
        <div className="toolbar">{actions}</div>
      </div>
      {children}
    </section>
  );
}
export function Empty({
  title,
  children,
}: {
  title: string;
  children?: ReactNode;
}) {
  useLocale();
  return (
    <div className="empty">
      <strong>{title}</strong>
      <div>{children}</div>
    </div>
  );
}
export function Modal({
  title,
  onClose,
  children,
  className,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}) {
  useLocale();
  const ref = useEntrance<HTMLDialogElement>();
  useEffect(() => {
    const dialog = ref.current;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog?.showModal();
    return () => {
      dialog?.close();
      if (opener?.isConnected) opener.focus();
    };
  }, []);
  return (
    <dialog
      className={className}
      ref={ref}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      aria-label={title}
    >
      <div className="modal-heading">
        <h2>{title}</h2>
        <Button
          aria-label={t("Close")}
          onClick={onClose}
          icon={<X size={16} />}
        />
      </div>
      {children}
    </dialog>
  );
}
export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  useLocale();
  return (
    <label className="field">
      <span>{label}</span>
      {isValidElement<{"aria-label"?: string}>(children) && children.type !== Fragment ? cloneElement(children, {"aria-label": children.props["aria-label"] ?? label}) : children}
    </label>
  );
}
export function Pending({ children }: { children: ReactNode }) {
  useLocale();
  return (
    <span className="pending" role="status">
      {children}
    </span>
  );
}
