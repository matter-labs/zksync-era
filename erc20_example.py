from flask import Flask
from random import randint

app = Flask(__name__)

@app.route("/conversion_rate/<token_address>")
def conversion_rate(token_address):
    print("Token address: ", token_address)
    return str(randint(1, 100))
