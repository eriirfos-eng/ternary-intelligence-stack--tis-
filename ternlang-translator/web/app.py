from flask import Flask, render_template

app = Flask(__name__)

@app.route('/')
def index():
    return render_template('index.html')

if __name__ == '__main__':
    print("🚀 Ternlang Translator Web UI starting at http://127.0.0.1:5000")
    app.run(debug=True, port=5000)
