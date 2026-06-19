import { Component, type ErrorInfo, type ReactNode } from "react";

export interface ErrorBoundaryProps {
  children: ReactNode;
  /** EN: Short heading for the fallback panel. CN: 回退面板标题。 */
  title: string;
  /** EN: Actionable hint (not raw stack traces). CN: 可操作提示（非原始堆栈）。 */
  hint: string;
  /** EN: Primary recovery action label. CN: 主恢复按钮文案。 */
  reloadLabel: string;
  /** EN: Optional secondary action (e.g. leave chat). CN: 可选次要动作（如离开会话）。 */
  secondaryLabel?: string;
  onSecondary?: () => void;
}

interface State {
  error: Error | null;
}

/// EN: Catch uncaught React render errors so the shell does not white-screen. CN: 捕获 React 渲染期未处理
/// 异常，避免整页白屏。
export class ErrorBoundary extends Component<ErrorBoundaryProps, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("[nexchat] UI error boundary:", error, info.componentStack);
  }

  private reload = (): void => {
    window.location.reload();
  };

  private reset = (): void => {
    this.setState({ error: null });
    this.props.onSecondary?.();
  };

  render(): ReactNode {
    if (!this.state.error) return this.props.children;
    return (
      <div className="tg-error-boundary" role="alert">
        <h2 className="tg-error-boundary-title">{this.props.title}</h2>
        <p className="tg-error-boundary-hint">{this.props.hint}</p>
        <div className="tg-error-boundary-actions">
          <button type="button" className="tg-offchain-sync-btn" onClick={this.reload}>
            {this.props.reloadLabel}
          </button>
          {this.props.secondaryLabel && this.props.onSecondary ? (
            <button type="button" className="tg-offchain-sync-btn" onClick={this.reset}>
              {this.props.secondaryLabel}
            </button>
          ) : null}
        </div>
      </div>
    );
  }
}
