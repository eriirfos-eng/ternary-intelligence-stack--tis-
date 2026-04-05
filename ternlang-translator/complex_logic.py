def process_signal(strength, context):
    if strength == True and context == True:
        return True
    elif strength == True and context is None:
        return True
    elif strength == False:
        return False
    else:
        return None
