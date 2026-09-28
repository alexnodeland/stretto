/**
 * The servers the mock's sessions talk to: stretto-mcp-demo's shop (its
 * tools and answers exactly as `stretto-mcp-demo --world retail` gives them),
 * the official filesystem server over someone's notes, and a ticket desk
 * over Streamable HTTP.
 */
import type { Result, ToolDef } from './recorder.ts'

export const SHOP_TOOLS: ToolDef[] = [
  {
    name: 'find_user_id_by_email',
    description: 'Find a user id by email.',
    readOnly: true,
    args: { email: '' },
    required: ['email'],
  },
  {
    name: 'get_user_details',
    description: "Get a user's details, including their orders.",
    readOnly: true,
    args: { user_id: '' },
    required: ['user_id'],
  },
  {
    name: 'get_order_details',
    description: 'Get the status and details of an order.',
    readOnly: true,
    args: { order_id: '' },
    required: ['order_id'],
  },
  {
    name: 'cancel_pending_order',
    description:
      "Cancel a pending order. The reason is 'no longer needed' or 'ordered by mistake'.",
    readOnly: false,
    args: { order_id: '', reason: '' },
    required: ['order_id', 'reason'],
  },
]

export const shop = {
  findUser: (n: number): Result => ({ text: `user_${n}` }),
  noUser: (email: string): Result => ({ text: `no user with email ${email}`, isError: true }),
  user: (n: number): Result => ({
    text: JSON.stringify({
      email: `c${n}@example.com`,
      orders: [`#W${n}a`, `#W${n}b`],
      payment_methods: { [`credit_card_${n}`]: { source: 'credit_card' } },
      user_id: `user_${n}`,
    }),
  }),
  order: (n: number, which: 'a' | 'b', status = 'pending'): Result => ({
    text: JSON.stringify({
      items: [{ item_id: '1', name: 'Desk lamp', product_id: 'p1' }],
      order_id: `#W${n}${which}`,
      payment_history: [{ payment_method_id: `credit_card_${n}` }],
      status,
      user_id: `user_${n}`,
    }),
  }),
  cancelled: (n: number, which: 'a' | 'b'): Result => ({
    text: JSON.stringify({ order_id: `#W${n}${which}`, status: 'cancelled' }),
  }),
}

export const FS_TOOLS: ToolDef[] = [
  {
    name: 'read_text_file',
    description: 'Read the complete contents of a file from the file system as text.',
    readOnly: true,
    args: { path: '' },
    required: ['path'],
  },
  {
    name: 'read_multiple_files',
    description: 'Read the contents of multiple files simultaneously.',
    readOnly: true,
    args: { paths: '' },
    required: ['paths'],
  },
  {
    name: 'list_directory',
    description: 'Get a detailed listing of all files and directories in a specified path.',
    readOnly: true,
    args: { path: '' },
    required: ['path'],
  },
  {
    name: 'directory_tree',
    description: 'Get a recursive tree view of files and directories as a JSON structure.',
    readOnly: true,
    args: { path: '' },
    required: ['path'],
  },
  {
    name: 'search_files',
    description: 'Recursively search for files and directories matching a pattern.',
    readOnly: true,
    args: { path: '', pattern: '' },
    required: ['path', 'pattern'],
  },
  {
    name: 'get_file_info',
    description: 'Retrieve detailed metadata about a file or directory.',
    readOnly: true,
    args: { path: '' },
    required: ['path'],
  },
  {
    name: 'list_allowed_directories',
    description: 'Returns the list of directories that this server is allowed to access.',
    readOnly: true,
    args: {},
    required: [],
  },
  {
    name: 'write_file',
    description: 'Create a new file or completely overwrite an existing file with new content.',
    readOnly: false,
    destructive: true,
    args: { path: '', content: '' },
    required: ['path', 'content'],
  },
  {
    name: 'edit_file',
    description: 'Make line-based edits to a text file.',
    readOnly: false,
    destructive: true,
    args: { path: '', edits: '' },
    required: ['path', 'edits'],
  },
  {
    name: 'create_directory',
    description: 'Create a new directory or ensure a directory exists.',
    readOnly: false,
    args: { path: '' },
    required: ['path'],
  },
  {
    name: 'move_file',
    description: 'Move or rename files and directories.',
    readOnly: false,
    args: { source: '', destination: '' },
    required: ['source', 'destination'],
  },
]

export const TICKET_TOOLS: ToolDef[] = [
  {
    name: 'search_tickets',
    description: 'Search tickets by text, status or requester.',
    readOnly: true,
    args: { query: '', status: '' },
    required: ['query'],
  },
  {
    name: 'get_ticket',
    description: 'Get a ticket with its comments.',
    readOnly: true,
    args: { ticket_id: '' },
    required: ['ticket_id'],
  },
  {
    name: 'get_requester',
    description: 'Get the person who opened a ticket.',
    readOnly: true,
    args: { requester_id: '' },
    required: ['requester_id'],
  },
  {
    name: 'add_comment',
    description: 'Add a public or internal comment to a ticket.',
    readOnly: false,
    args: { ticket_id: '', body: '', internal: '' },
    required: ['ticket_id', 'body'],
  },
  {
    name: 'set_status',
    description: "Set a ticket's status.",
    readOnly: false,
    args: { ticket_id: '', status: '' },
    required: ['ticket_id', 'status'],
  },
  {
    name: 'escalate',
    description: 'Escalate a ticket to the on-call engineer.',
    readOnly: null,
    args: { ticket_id: '', reason: '' },
    required: ['ticket_id'],
  },
]
