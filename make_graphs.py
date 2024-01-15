import matplotlib.pyplot as plt
import pandas as pd

data = pd.read_csv('report.csv')
df = pd.DataFrame(data)

mint_data = df[df['operation'] == 'Mint']
transfer_data = df[df['operation'] == 'Transfer']

mint_average = mint_data['l2_fee'].mean()
transfer_average = transfer_data['l2_fee'].mean()

plt.bar(['Mint', 'Transfer'], [mint_average, transfer_average])
plt.savefig('graph_report.png')
