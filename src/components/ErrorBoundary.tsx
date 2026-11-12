// Ошибка в одной карточке не должна ронять всю панель.

import { Component, type ReactNode } from "react";

export class ErrorBoundary extends Component<{ title: string; children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null };

  static getDerivedStateFromError(e: unknown) {
    return { error: e instanceof Error ? e.message : String(e) };
  }

  componentDidCatch(e: unknown) {
    console.error(`card ${this.props.title} crashed`, e);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="card-pad">
          <div className="note note-error">
            Карточка «{this.props.title}» завершилась с ошибкой: {this.state.error}
            <div>
              <button type="button" className="btn btn-sm" onClick={() => this.setState({ error: null })}>
                Повторить
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
