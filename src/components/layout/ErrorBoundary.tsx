import { Component } from "react";
import type { ErrorInfo, ReactNode } from "react";
import { QueryClient } from "@tanstack/react-query";

interface Props {
  children: ReactNode;
  queryClient?: QueryClient;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("Uncaught error:", error, errorInfo);
  }

  handleReset = () => {
    if (this.props.queryClient) {
      this.props.queryClient.clear();
    }
    this.setState({ hasError: false, error: null });
  };

  handleReturnHome = () => {
    window.location.assign("/");
  };

  render() {
    if (this.state.hasError) {
      return (
        <main className="bg-surface flex min-h-screen items-center justify-center p-8">
          <section
            aria-labelledby="error-boundary-title"
            aria-describedby="error-boundary-message"
            className="border-border bg-surface-alt w-full max-w-md rounded-lg border p-8 text-center shadow-xl"
            role="alert"
          >
            <p className="text-danger mb-2 text-xs font-semibold tracking-wide uppercase">
              Recovery needed
            </p>
            <h1 id="error-boundary-title" className="text-text mb-3 text-2xl font-bold">
              Something went wrong
            </h1>
            <p id="error-boundary-message" className="text-text-muted mb-6 text-sm leading-6">
              {this.state.error?.message ?? "An unexpected error occurred."}
            </p>
            <div className="flex flex-col justify-center gap-3 sm:flex-row">
              <button
                onClick={this.handleReset}
                className="bg-primary hover:bg-primary-dark focus-visible:outline-primary-light rounded-lg px-4 py-2 text-sm font-medium text-white transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2"
              >
                Try again
              </button>
              <button
                onClick={this.handleReturnHome}
                className="border-border bg-surface text-text hover:bg-surface-hover focus-visible:outline-primary-light rounded-lg border px-4 py-2 text-sm font-medium transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2"
              >
                Back to dashboard
              </button>
            </div>
          </section>
        </main>
      );
    }

    return this.props.children;
  }
}
