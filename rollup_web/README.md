# ZKSVM Rollup Web Client

A modern web interface for the ZKSVM rollup client built with Next.js 15, TypeScript, and Tailwind CSS.

## Features

- **Health Monitoring**: Check rollup server connectivity and status
- **Transaction Creation**: Create and submit new transactions to the rollup
- **Transaction Search**: Search for specific transactions by signature hash
- **Transaction Listing**: Browse recent transactions with pagination
- **Real-time Updates**: Automatic refresh and real-time status updates
- **Responsive Design**: Modern, clean interface inspired by shadcn/ui

## Getting Started

### Prerequisites

- Node.js 18+
- npm or yarn
- Running ZKSVM rollup core server (default: `http://127.0.0.1:8080`)

### Installation

1. Install dependencies:

```bash
npm install
```

2. Set up environment variables:
   Create a `.env.local` file in the root directory:

```env
NEXT_PUBLIC_ROLLUP_URL=http://127.0.0.1:8080
```

3. Start the development server:

```bash
npm run dev
```

4. Open [http://localhost:3000](http://localhost:3000) in your browser.

## Usage

### Health Check

- Click "Check Health" to verify connectivity to the rollup server
- View server response and timestamp of last check

### Create Transaction

- Fill in sender name, recipient address, and amount
- Click "Submit Transaction" to send to the rollup
- View transaction submission results

### Search Transactions

- Enter a transaction signature hash in the search field
- Click "Search" or press Enter to find the specific transaction
- View detailed transaction information

### Browse Transactions

- View recent transactions in the main list
- Use pagination controls to navigate through transaction history
- Click "Refresh" to update the transaction list

## API Integration

The web interface communicates with the rollup core server through HTTP endpoints:

- `GET /` - Health check
- `POST /submit_transaction` - Submit new transactions
- `POST /get_transaction` - Retrieve specific transactions or paginated lists

## Development

### Project Structure

```
rollup_web/
├── app/                    # Next.js app directory
│   ├── globals.css        # Global styles with Tailwind
│   ├── layout.tsx         # Root layout component
│   └── page.tsx           # Main rollup client page
├── components/            # Reusable UI components
│   ├── ui/               # Base UI components (button, card, input)
│   └── transaction-creator.tsx # Transaction creation form
├── lib/                   # Utility functions and API client
│   ├── api.ts            # Rollup server API functions
│   └── utils.ts          # Helper utilities
└── public/               # Static assets
```

### Styling

The project uses Tailwind CSS v4 with a design system inspired by shadcn/ui:

- Consistent color palette with dark mode support
- Responsive design patterns
- Accessible UI components
- Modern shadows and borders

### Type Safety

All components and API functions are fully typed with TypeScript:

- Strict type checking for rollup transaction structures
- Interface definitions for all API responses
- Type-safe React components with proper prop validation

## Building for Production

```bash
npm run build
npm start
```

## Configuration

### Environment Variables

| Variable                 | Description       | Default                 |
| ------------------------ | ----------------- | ----------------------- |
| `NEXT_PUBLIC_ROLLUP_URL` | Rollup server URL | `http://127.0.0.1:8080` |

### Rollup Server

Ensure the rollup core server is running and accessible. The web client expects these endpoints:

- Health check: `GET /`
- Submit transaction: `POST /submit_transaction`
- Get transaction(s): `POST /get_transaction`

## Contributing

1. Follow the existing code style and patterns
2. Add proper TypeScript types for new features
3. Test functionality with the rollup core server
4. Update documentation for new features

## License

This project is part of the ZKSVM rollup system.
