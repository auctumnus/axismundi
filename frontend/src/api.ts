export interface User {
  id: string;
  name: string;
  email: string;
  created_at: string;
}

export interface CreateUser {
  name: string;
  email: string;
}

const API_BASE = '/api';

export class ApiClient {
  async getUsers(): Promise<User[]> {
    const response = await fetch(`${API_BASE}/users`);
    if (!response.ok) throw new Error('Failed to fetch users');
    return response.json();
  }

  async getUser(id: string): Promise<User> {
    const response = await fetch(`${API_BASE}/users/${id}`);
    if (!response.ok) throw new Error('Failed to fetch user');
    return response.json();
  }

  async createUser(user: CreateUser): Promise<User> {
    const response = await fetch(`${API_BASE}/users`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(user)
    });
    if (!response.ok) throw new Error('Failed to create user');
    return response.json();
  }
}

export const api = new ApiClient();