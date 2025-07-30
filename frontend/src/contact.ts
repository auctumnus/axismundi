import '../styles/main.css';

interface ContactMessage {
  name: string;
  email: string;
  subject: string;
  message: string;
}

class ContactPage {
  private contactForm: HTMLFormElement;
  private statusContainer: HTMLElement;

  constructor() {
    this.contactForm = document.getElementById('contactForm') as HTMLFormElement;
    this.statusContainer = document.getElementById('contactStatus')!;
    
    this.setupEventListeners();
  }

  private setupEventListeners() {
    this.contactForm.addEventListener('submit', this.handleSubmit.bind(this));
  }

  private async handleSubmit(event: Event) {
    event.preventDefault();
    
    const nameInput = document.getElementById('contactName') as HTMLInputElement;
    const emailInput = document.getElementById('contactEmail') as HTMLInputElement;
    const subjectInput = document.getElementById('contactSubject') as HTMLInputElement;
    const messageInput = document.getElementById('contactMessage') as HTMLTextAreaElement;
    
    const contactMessage: ContactMessage = {
      name: nameInput.value,
      email: emailInput.value,
      subject: subjectInput.value,
      message: messageInput.value
    };

    try {
      // Simulate sending the message (you'd implement this API endpoint)
      await this.sendMessage(contactMessage);
      
      // Clear form
      this.contactForm.reset();
      
      // Show success message
      this.showStatus('Message sent successfully! We\'ll get back to you soon.', 'success');
    } catch (error) {
      console.error('Error sending message:', error);
      this.showStatus('Failed to send message. Please try again.', 'error');
    }
  }

  private async sendMessage(message: ContactMessage): Promise<void> {
    // This would be implemented as an actual API endpoint
    // For now, just simulate the request
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        // Simulate success/failure
        if (Math.random() > 0.1) {
          resolve();
        } else {
          reject(new Error('Simulated error'));
        }
      }, 1000);
    });
  }

  private showStatus(message: string, type: 'success' | 'error') {
    this.statusContainer.innerHTML = `
      <div style="
        padding: 10px; 
        margin: 10px 0; 
        border-radius: 4px; 
        color: white;
        background-color: ${type === 'success' ? '#28a745' : '#dc3545'};
      ">
        ${message}
      </div>
    `;
    
    // Clear status after 5 seconds
    setTimeout(() => {
      this.statusContainer.innerHTML = '';
    }, 5000);
  }
}

document.addEventListener('DOMContentLoaded', () => {
  new ContactPage();
});