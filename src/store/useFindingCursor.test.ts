import { beforeEach, describe, expect, it } from "vitest";

import { useFindingCursor } from "./useFindingCursor";

const get = () => useFindingCursor.getState();

describe("useFindingCursor", () => {
  beforeEach(() => {
    useFindingCursor.setState({ cursor: -1, count: 0 });
  });

  it("does nothing when the list is empty", () => {
    get().step(1);
    expect(get().cursor).toBe(-1);
  });

  it("starts at the top going forward and the bottom going back", () => {
    get().setCount(5);
    get().step(1);
    expect(get().cursor).toBe(0);

    useFindingCursor.setState({ cursor: -1 });
    get().step(-1);
    expect(get().cursor).toBe(4);
  });

  it("clamps rather than wrapping at either end", () => {

    get().setCount(3);
    useFindingCursor.setState({ cursor: 2 });
    get().step(1);
    expect(get().cursor).toBe(2);

    useFindingCursor.setState({ cursor: 0 });
    get().step(-1);
    expect(get().cursor).toBe(0);
  });

  it("pulls the cursor back when the list gets shorter", () => {

    get().setCount(10);
    useFindingCursor.setState({ cursor: 8 });
    get().setCount(3);
    expect(get().cursor).toBe(2);
  });

  it("leaves the cursor alone when the list is still long enough", () => {
    get().setCount(10);
    useFindingCursor.setState({ cursor: 2 });
    get().setCount(5);
    expect(get().cursor).toBe(2);
  });

  it("goes back to nothing selected on reset", () => {
    get().setCount(4);
    useFindingCursor.setState({ cursor: 3 });
    get().reset();
    expect(get().cursor).toBe(-1);
  });
});
