from flask import Flask
from random import randint

app = Flask(__name__)

@app.route("/conversion_rate")
def conversion_rate():
    return str(randint(1, 100))
