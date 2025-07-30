import { api, User, CreateUser } from './api';

class HomePage {
  private usersContainer: HTMLElement;
  private userForm: HTMLFormElement;

  constructor() {
    this.usersContainer = document.getElementById('users')!;
    this.userForm = document.getElementById('userForm') as HTMLFormElement;
    
    this.setupEventListeners();
    this.loadUsers();
  }

  private setupEventListeners() {
    this.userForm.addEventListener('submit', this.handleSubmit.bind(this));
  }

  private async handleSubmit(event: Event) {
    event.preventDefault();
    
    const nameInput = document.getElementById('userName') as HTMLInputElement;
    const emailInput = document.getElementById('userEmail') as HTMLInputElement;
    
    const newUser: CreateUser = {
      name: nameInput.value,
      email: emailInput.value
    };

    try {
      await api.createUser(newUser);
      nameInput.value = '';
      emailInput.value = '';
      this.loadUsers(); // Refresh the list
    } catch (error) {
      console.error('Error creating user:', error);
      alert('Failed to create user');
    }
  }

  private async loadUsers() {
    try {
      const users = await api.getUsers();
      this.renderUsers(users);
    } catch (error) {
      console.error('Error loading users:', error);
      this.usersContainer.innerHTML = '<p>Failed to load users</p>';
    }
  }

  private renderUsers(users: User[]) {
    if (users.length === 0) {
      this.usersContainer.innerHTML = '<p>No users found</p>';
      return;
    }

    this.usersContainer.innerHTML = users.map(user => `
      <div class="user-item">
        <strong>${user.name}</strong> (${user.email})
        <br><small>Created: ${new Date(user.created_at).toLocaleDateString()}</small>
      </div>
    `).join('');
  }
}

// Initialize the page when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
  new HomePage();
});