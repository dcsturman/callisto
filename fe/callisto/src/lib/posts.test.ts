import { describe, it, expect } from "vitest";
import { EntityList } from "lib/entities";
import { fromPost, postsToDraw } from "lib/posts";

const world = (ships: object[]): EntityList =>
  ({
    ships,
    planets: [{ name: "Europa", position: [0, 0, 6.71e8] }],
    missiles: [],
  }) as unknown as EntityList;

describe("postsToDraw", () => {
  it("draws a side's post once, at the body it names", () => {
    const posts = postsToDraw(
      world([
        { name: "Ailreaui", team: "Red", goal: "hold", post: "Europa" },
        { name: "AF1", team: "Red", goal: "hold", post: "Europa" },
      ]),
    );
    expect(posts).toHaveLength(1);
    expect(posts[0].position).toEqual([0, 0, 6.71e8]);
    expect(posts[0].radius).toBe(25_000_000);
    expect(posts[0].label).toBe("Red's post: Europa");
  });

  it("draws a point post with its own radius", () => {
    const [post] = postsToDraw(
      world([{ name: "Vigilant", team: "Blue", goal: "hold", post: [0, 0, 1e8], post_radius: 10_000 }]),
    );
    expect(post.position).toEqual([0, 0, 1e8]);
    expect(post.radius).toBe(10_000_000);
  });

  it("draws nothing for a ship that is not holding, or names no post", () => {
    expect(
      postsToDraw(world([{ name: "Unicorn", team: "Blue", goal: "destroy" }, { name: "SDB", goal: "hold" }])),
    ).toHaveLength(0);
  });
});

describe("fromPost", () => {
  const ship = (position: [number, number, number]) =>
    ({ name: "AF1", team: "Red", goal: "hold", post: "Europa", position }) as never;

  it("says how far a Hold ship is from her post, and whether that is inside", () => {
    const near = fromPost(world([]), ship([0, 0, 6.71e8 - 10_000_000]));
    expect(near?.inside).toBe(true);
    expect(Math.round(near!.distance)).toBe(10_000_000);
    expect(fromPost(world([]), ship([0, 0, 6.71e8 - 40_000_000]))?.inside).toBe(false);
  });

  it("is nothing for a ship that holds no post", () => {
    expect(fromPost(world([]), { name: "Unicorn", goal: "destroy", position: [0, 0, 0] } as never)).toBeNull();
  });
});
