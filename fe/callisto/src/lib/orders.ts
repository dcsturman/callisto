/**
 * Whether players can write a ship's standing orders.
 *
 * Off for now: the AI crews no longer read them, and nothing else does, so a
 * box to write them in only invites orders nobody follows. They come back
 * when an AI reads them again. The orders a scenario file already carries
 * are kept, just not shown.
 */
export const ORDERS_ENABLED = false;
